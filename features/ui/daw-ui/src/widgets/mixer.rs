//! Mixer channel strip, mixer buttons, and a folder-hierarchy mixer.
//!
//! A Reaper-MCP-style vertical [`ChannelStrip`]: a track-coloured strip with a
//! pan knob, [`SoloButton`] / [`MuteButton`], a **fader whose draggable cap
//! overlays an integrated level meter**, a [`RoutingButton`] (three status
//! stripes), and the track name as a footer.
//!
//! [`MixerFolder`] groups strips under a coloured header. With the `hierarchy`
//! feature, [`Mixer`] renders a flat track list with relative folder-depth
//! changes (`daw_proto::track::FolderDepthChange`) into nested folders —
//! consuming the daw's hierarchy model directly.

use crate::core::sensitivity::{DragSensitivity, ModifierKeys};
use crate::prelude::*;
use crate::theming::{Color, ThemeState, ToggleKind, use_theme};
use crate::widgets::knob::{Knob, KnobVariant};

// Colours are theme-driven (see `crate::theming`); these helpers remain for
// `MixerFolder`'s track-colour handling.

fn parse_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&h[0..2], 16).ok()?,
        u8::from_str_radix(&h[2..4], 16).ok()?,
        u8::from_str_radix(&h[4..6], 16).ok()?,
    ))
}

/// Near-black or near-white text colour for legibility over `hex`.
fn readable_on(hex: &str) -> &'static str {
    match parse_rgb(hex) {
        Some((r, g, b)) => {
            let lum = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;
            if lum > 0.6 { "#0c0c0f" } else { "#f4f4f5" }
        }
        None => "#f4f4f5",
    }
}

/// Append an 8-bit alpha (e.g. `"22"`) to a `#rrggbb`. Unchanged if not 7-char.
fn with_alpha(hex: &str, aa: &str) -> String {
    if hex.len() == 7 && hex.starts_with('#') {
        format!("{hex}{aa}")
    } else {
        hex.to_string()
    }
}

/// Resolve a caller colour to `#rrggbb`, falling back to a neutral accent.
fn track_hex(color: &Option<String>) -> String {
    color
        .as_deref()
        .filter(|c| parse_rgb(c).is_some())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "#a1a1aa".to_string())
}

// ── Toggle buttons (Solo / Mute) ──────────────────────────────────────────────

/// Shared square toggle behind [`SoloButton`] / [`MuteButton`]. Colours come
/// from `theme.toggle(kind)`.
#[component]
fn MixerToggleButton(
    active: Signal<bool>,
    kind: ToggleKind,
    #[props(default)] disabled: bool,
    #[props(default)] on_toggle: Option<Callback<bool>>,
) -> Element {
    let t = use_theme().theme.toggle(kind);
    let (glyph, title) = match kind {
        ToggleKind::Solo => ("S", "Solo"),
        ToggleKind::Mute => ("M", "Mute"),
        ToggleKind::RecArm => ("\u{25cf}", "Record arm"),
    };
    let on = active();
    let cursor = if disabled { "not-allowed" } else { "pointer" };
    let style = if on {
        format!(
            "flex:1; height:22px; display:flex; align-items:center; justify-content:center; \
             font-size:12px; font-weight:800; line-height:1; border-radius:5px; cursor:{cursor}; \
             background:{fill}; color:{fg}; border:1px solid {fill}; \
             box-shadow:0 0 8px {glow}, inset 0 1px 0 rgba(255,255,255,0.3);",
            fill = t.on_fill.css(),
            fg = t.on_text.css(),
            glow = t.on_fill.with_alpha(0x99).css(),
        )
    } else {
        format!(
            "flex:1; height:22px; display:flex; align-items:center; justify-content:center; \
             font-size:12px; font-weight:800; line-height:1; border-radius:5px; cursor:{cursor}; \
             background:{bg}; color:{txt}; border:1px solid {border};",
            bg = t.off_bg.css(),
            txt = t.off_text.css(),
            border = t.off_border.css(),
        )
    };
    rsx! {
        button {
            r#type: "button",
            title,
            style,
            onclick: move |_| {
                if disabled { return; }
                let next = !active();
                active.set(next);
                if let Some(cb) = on_toggle { cb.call(next); }
            },
            "{glyph}"
        }
    }
}

/// Solo button — toggles a `bool` signal; amber when active.
#[component]
pub fn SoloButton(
    active: Signal<bool>,
    #[props(default)] disabled: bool,
    #[props(default)] on_toggle: Option<Callback<bool>>,
) -> Element {
    rsx! {
        MixerToggleButton { active, kind: ToggleKind::Solo, disabled, on_toggle }
    }
}

/// Mute button — toggles a `bool` signal; red when active.
#[component]
pub fn MuteButton(
    active: Signal<bool>,
    #[props(default)] disabled: bool,
    #[props(default)] on_toggle: Option<Callback<bool>>,
) -> Element {
    rsx! {
        MixerToggleButton { active, kind: ToggleKind::Mute, disabled, on_toggle }
    }
}

/// Routing button — a square icon with three diagonal stripes. Each lights when
/// its state is on: **parent send** (green), **sending** (yellow), **receiving**
/// (blue). Clicking fires `on_click`.
#[component]
pub fn RoutingButton(
    #[props(default)] parent_send: bool,
    #[props(default)] sends: bool,
    #[props(default)] receives: bool,
    #[props(default)] disabled: bool,
    #[props(default)] on_click: Option<Callback<()>>,
) -> Element {
    let tk = use_theme().theme.tokens;
    let off = tk.border;
    let cursor = if disabled { "not-allowed" } else { "pointer" };
    let c_parent = if parent_send { tk.route_parent } else { off }.css();
    let c_send = if sends { tk.route_send } else { off }.css();
    let c_recv = if receives { tk.route_recv } else { off }.css();
    let bg = tk.surface_sunken.css();
    let border = tk.border.css();
    rsx! {
        button {
            r#type: "button",
            title: "Routing",
            style: format!(
                "width:26px; height:26px; padding:0; display:flex; align-items:center; \
                 justify-content:center; border-radius:5px; cursor:{cursor}; \
                 background:{bg}; border:1px solid {border};"
            ),
            onclick: move |_| {
                if disabled { return; }
                if let Some(cb) = on_click { cb.call(()); }
            },
            svg {
                width: "18",
                height: "18",
                view_box: "0 0 24 24",
                line { x1: "2", y1: "11", x2: "11", y2: "2", stroke: "{c_parent}", stroke_width: "3", stroke_linecap: "round" }
                line { x1: "7", y1: "17", x2: "17", y2: "7", stroke: "{c_send}", stroke_width: "3", stroke_linecap: "round" }
                line { x1: "13", y1: "22", x2: "22", y2: "13", stroke: "{c_recv}", stroke_width: "3", stroke_linecap: "round" }
            }
        }
    }
}

// ── Fader + integrated meter ──────────────────────────────────────────────────

/// Vertical fader whose draggable (neutral) cap overlays an integrated level
/// meter. `value` is the normalized fader position (0–1); `level`/`level_right`
/// are the meter levels; `peak` an optional hold marker. The cap's centre line
/// is painted `accent` (the track colour).
#[component]
pub fn ChannelFader(
    value: Signal<f32>,
    #[props(default = 0.0)] level: f32,
    #[props(default)] level_right: Option<f32>,
    #[props(default)] peak: Option<f32>,
    #[props(default = "#a1a1aa".to_string())] accent: String,
    #[props(default = 180)] height: u32,
    #[props(default)] disabled: bool,
    #[props(default)] on_change: Option<Callback<f32>>,
) -> Element {
    let mut is_dragging = use_signal(|| false);
    let mut drag_start_y = use_signal(|| 0.0f32);
    let mut drag_start_value = use_signal(|| 0.0f32);

    let sensitivity = DragSensitivity::new(height as f32, 0.1);
    let v = value().clamp(0.0, 1.0);
    let fill_pct = v * 100.0;
    let cursor = if disabled { "not-allowed" } else { "ns-resize" };

    let theme = use_theme().theme;
    let m = theme.meter();
    let f = theme.fader(&ThemeState::new());
    let well = m.well.css();
    let well_border = m.border.css();
    let peak_col = m.peak.css();
    let cap_top = f.cap_top.css();
    let cap_bottom = f.cap_bottom.css();

    // Meter columns: stereo (two) or mono (one). Zone colour comes from the theme.
    // A silent (or non-finite) level renders nothing: a zero-height rounded rect
    // would make vello's path flattener emit NaN-path warnings every frame.
    let meter = |lvl: f32, left: f32, w: f32| {
        let lvl = if lvl.is_finite() {
            lvl.clamp(0.0, 1.0)
        } else {
            0.0
        };
        rsx! {
            if lvl > 0.0 {
                div {
                    style: format!(
                        "position:absolute; bottom:0; left:{left}%; width:{w}%; height:{h}%; \
                         background:{c}; opacity:0.9; pointer-events:none; border-radius:1px;",
                        h = lvl * 100.0,
                        c = theme.meter_zone(lvl).css(),
                    ),
                }
            }
        }
    };

    rsx! {
        div {
            style: format!(
                "position:relative; width:30px; height:{height}px; background:{well}; \
                 border-radius:5px; overflow:hidden; border:1px solid {well_border}; cursor:{cursor}; \
                 box-shadow:inset 0 1px 3px rgba(0,0,0,0.5);"
            ),

            if let Some(right) = level_right {
                {meter(level, 10.0, 35.0)}
                {meter(right, 55.0, 35.0)}
            } else {
                {meter(level, 14.0, 72.0)}
            }

            if let Some(peak) = peak {
                div {
                    style: format!(
                        "position:absolute; left:0; right:0; bottom:{p}%; height:2px; \
                         background:{peak_col}; pointer-events:none;",
                        p = peak.clamp(0.0, 1.0) * 100.0,
                    ),
                }
            }

            // Fader cap.
            div {
                style: format!(
                    "position:absolute; left:-3px; right:-3px; bottom:calc({fill_pct}% - 6px); \
                     height:12px; border-radius:3px; \
                     background:linear-gradient(180deg,{cap_top},{cap_bottom}); \
                     border:1px solid #15151a; pointer-events:none; \
                     box-shadow:0 1px 2px rgba(0,0,0,0.6);"
                ),
                div {
                    style: format!(
                        "position:absolute; left:3px; right:3px; top:5px; height:2px; \
                         background:{accent}; border-radius:1px;"
                    ),
                }
            }

            // Drag overlay (vertical drag; up = increase).
            div {
                style: "position:absolute; inset:0;",
                onmousedown: move |evt: MouseEvent| {
                    if disabled { return; }
                    is_dragging.set(true);
                    drag_start_y.set(evt.client_coordinates().y as f32);
                    drag_start_value.set(value().clamp(0.0, 1.0));
                },
                onmousemove: move |evt: MouseEvent| {
                    if !*is_dragging.read() || disabled { return; }
                    let delta_y = drag_start_y() - evt.client_coordinates().y as f32;
                    let modifiers = ModifierKeys::new(
                        evt.modifiers().shift(), evt.modifiers().ctrl(), evt.modifiers().alt(),
                    );
                    let delta = sensitivity.calculate_delta(-delta_y, modifiers);
                    let next = (drag_start_value() + delta).clamp(0.0, 1.0);
                    value.set(next);
                    if let Some(cb) = on_change { cb.call(next); }
                },
                onmouseup: move |_| { is_dragging.set(false); },
                onmouseleave: move |_| { is_dragging.set(false); },
            }
        }
    }
}

// ── Channel strip ──────────────────────────────────────────────────────────────

/// A mixer channel strip. Track-coloured, with a pan knob, Solo/Mute, a
/// fader+meter, a routing button, and the name as a footer.
#[component]
pub fn ChannelStrip(
    /// Normalized fader position (0–1).
    fader: Signal<f32>,
    /// Mute state.
    mute: Signal<bool>,
    /// Solo state.
    solo: Signal<bool>,
    /// Optional pan (bipolar; 0.5 = centre).
    #[props(default)]
    pan: Option<Signal<f32>>,
    #[props(default)] name: Option<String>,
    /// Track colour (`#rrggbb`). Tints the container and footer.
    #[props(default)]
    color: Option<String>,
    #[props(default = 0.0)] level: f32,
    #[props(default)] level_right: Option<f32>,
    #[props(default)] peak: Option<f32>,
    #[props(default = true)] parent_send: bool,
    #[props(default)] sends: bool,
    #[props(default)] receives: bool,
    #[props(default)] on_routing: Option<Callback<()>>,
    #[props(default = 64)] width: u32,
    #[props(default = 180)] fader_height: u32,
    #[props(default)] disabled: bool,
) -> Element {
    let theme = use_theme().theme;
    let st = ThemeState::new().track(color.as_deref().and_then(Color::hex));
    let s = theme.strip(&st);
    let surface = theme.tokens.surface.css();
    let track = s.accent.css();
    let footer_fg = s.footer_text.css();
    let body_tint = s.body.css();
    let border_col = s.border.css();
    let value_fg = s.value_text.css();
    let opacity = if disabled { "0.5" } else { "1.0" };
    let title = name.clone().unwrap_or_default();
    let display = format!("{:.0}%", fader().clamp(0.0, 1.0) * 100.0);

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:stretch; \
                 width:{width}px; overflow:hidden; margin-right:-1px; opacity:{opacity}; \
                 border:1px solid {border_col}; background:{surface}; user-select:none;"
            ),

            // Body.
            div {
                style: format!(
                    "display:flex; flex-direction:column; gap:8px; padding:8px; background:{body_tint};"
                ),

                if let Some(pan) = pan {
                    div {
                        style: "display:flex; justify-content:center;",
                        Knob { value: pan, min: 0.0, max: 1.0, default: 0.5, variant: KnobVariant::ArcBipolar, size: 34, disabled }
                    }
                }

                div {
                    style: "display:flex; gap:5px;",
                    SoloButton { active: solo, disabled }
                    MuteButton { active: mute, disabled }
                }

                div {
                    style: "display:flex; justify-content:center;",
                    ChannelFader {
                        value: fader,
                        level,
                        level_right,
                        peak,
                        accent: track.clone(),
                        height: fader_height,
                        disabled,
                    }
                }

                span {
                    style: format!(
                        "font-size:10px; color:{value_fg}; text-align:center; font-variant-numeric:tabular-nums;"
                    ),
                    "{display}"
                }

                if let Some(handler) = on_routing {
                    div {
                        style: "display:flex; justify-content:center;",
                        RoutingButton {
                            parent_send,
                            sends,
                            receives,
                            disabled,
                            on_click: move |_| handler.call(()),
                        }
                    }
                }
            }

            // Footer: track name in the track colour.
            div {
                style: format!(
                    "padding:5px 8px; background:{track}; color:{footer_fg}; font-size:11px; \
                     font-weight:700; letter-spacing:0.02em; white-space:nowrap; overflow:hidden; \
                     text-overflow:ellipsis; text-align:center;"
                ),
                "{title}"
            }
        }
    }
}

/// Groups channel strips under a coloured folder header. Pass child strips as
/// `children`; nest a `MixerFolder` for deeper hierarchy.
#[component]
pub fn MixerFolder(
    name: String,
    #[props(default)] color: Option<String>,
    children: Element,
) -> Element {
    let folder = track_hex(&color);
    let fg = readable_on(&folder);
    let tint = with_alpha(&folder, "1f");
    rsx! {
        div {
            style: "display:inline-flex; flex-direction:column; align-items:stretch; margin-right:-1px;",
            div {
                style: format!(
                    "padding:4px 8px; background:{folder}; color:{fg}; font-size:10px; \
                     font-weight:800; text-transform:uppercase; letter-spacing:0.05em; \
                     display:flex; align-items:center; gap:5px; border:1px solid {folder};"
                ),
                span { style: "font-size:9px; opacity:0.85;", "▼" }
                span { style: "white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{name}" }
            }
            div {
                style: format!(
                    "display:inline-flex; align-items:flex-start; gap:0; padding:4px; \
                     background:{tint}; border:1px solid {folder}; border-top:none;"
                ),
                {children}
            }
        }
    }
}

// ── Hierarchy-driven mixer (uses daw-proto track types directly) ───────────────
#[cfg(feature = "hierarchy")]
mod hierarchy {
    use super::*;
    use daw_proto::track::FolderDepthChange;

    /// One track in a [`Mixer`], carrying its live signals + folder-depth change.
    /// `depth_change` is `daw_proto`'s relative folder-depth model, consumed
    /// directly — `FolderStart` opens a folder, `ClosesLevels(n)` closes `n`.
    #[derive(Clone, PartialEq)]
    pub struct MixerStrip {
        pub name: String,
        pub color: Option<String>,
        pub depth_change: FolderDepthChange,
        pub fader: Signal<f32>,
        pub mute: Signal<bool>,
        pub solo: Signal<bool>,
        pub pan: Option<Signal<f32>>,
        pub level: f32,
        pub level_right: Option<f32>,
        pub peak: Option<f32>,
        pub parent_send: bool,
        pub sends: bool,
        pub receives: bool,
        pub disabled: bool,
    }

    enum Node {
        Leaf(MixerStrip),
        Folder {
            strip: MixerStrip,
            children: Vec<Node>,
        },
    }

    /// Parse the flat depth-change list into a nested tree (Reaper semantics:
    /// a track's change is relative to the previous track).
    fn parse(tracks: &[MixerStrip]) -> Vec<Node> {
        let mut roots: Vec<Node> = Vec::new();
        let mut stack: Vec<(MixerStrip, Vec<Node>)> = Vec::new();

        let push_node =
            |node: Node, stack: &mut Vec<(MixerStrip, Vec<Node>)>, roots: &mut Vec<Node>| {
                match stack.last_mut() {
                    Some((_, children)) => children.push(node),
                    None => roots.push(node),
                }
            };

        for t in tracks {
            if t.depth_change.is_folder_start() {
                stack.push((t.clone(), Vec::new()));
            } else {
                push_node(Node::Leaf(t.clone()), &mut stack, &mut roots);
                for _ in 0..t.depth_change.levels_closed() {
                    if let Some((strip, children)) = stack.pop() {
                        push_node(Node::Folder { strip, children }, &mut stack, &mut roots);
                    }
                }
            }
        }
        while let Some((strip, children)) = stack.pop() {
            push_node(Node::Folder { strip, children }, &mut stack, &mut roots);
        }
        roots
    }

    fn render_strip(s: &MixerStrip) -> Element {
        rsx! {
            ChannelStrip {
                fader: s.fader,
                mute: s.mute,
                solo: s.solo,
                pan: s.pan,
                name: s.name.clone(),
                color: s.color.clone(),
                level: s.level,
                level_right: s.level_right,
                peak: s.peak,
                parent_send: s.parent_send,
                sends: s.sends,
                receives: s.receives,
                disabled: s.disabled,
                on_routing: move |_| {},
            }
        }
    }

    fn render_node(node: &Node) -> Element {
        match node {
            Node::Leaf(s) => render_strip(s),
            Node::Folder { strip, children } => {
                let kids: Vec<Element> = children.iter().map(render_node).collect();
                rsx! {
                    MixerFolder {
                        name: strip.name.clone(),
                        color: strip.color.clone(),
                        {render_strip(strip)}
                        for kid in kids {
                            {kid}
                        }
                    }
                }
            }
        }
    }

    /// Renders a flat track list (with relative `FolderDepthChange`s) into a
    /// nested mixer console — folders within folders to arbitrary depth.
    #[component]
    pub fn Mixer(tracks: Vec<MixerStrip>) -> Element {
        let nodes = parse(&tracks);
        let rendered: Vec<Element> = nodes.iter().map(render_node).collect();
        rsx! {
            div {
                style: "display:inline-flex; align-items:flex-start; padding:8px;",
                for node in rendered {
                    {node}
                }
            }
        }
    }
}

#[cfg(feature = "hierarchy")]
pub use hierarchy::{Mixer, MixerStrip};
