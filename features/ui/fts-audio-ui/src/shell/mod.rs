//! The FTS plugin shell — one chrome for every FTS plugin editor.
//!
//! Every FTS editor is the same shape: a plugin identity, a list of
//! *profiles* (the hardware unit or mode the plugin is emulating), and one
//! big working surface. Before this each editor grew its own header strip,
//! which meant every plugin looked slightly different and every plugin spent
//! vertical pixels — the scarce ones — on chrome.
//!
//! [`PluginShell`] puts the chrome in a slim icon rail instead — the same
//! ribbon the Task desktop app wears, so the FTS apps and the FTS plugins
//! read as one product:
//!
//! ```text
//! ┌────┬─────────────────────────────────────┐
//! │ FC │                                     │
//! ├────┤                                     │
//! │CTL │                                     │
//! │ 2A │         the working surface         │
//! │SSL │         (graph, faceplate, …)       │
//! │ 76 │                                     │
//! │    │                                     │
//! ├────┤                                     │
//! │ADV │                                     │
//! └────┴─────────────────────────────────────┘
//! ```
//!
//! Three things follow, and they are the point:
//!
//! - **The surface gets the whole window.** A graph or a faceplate is limited
//!   by height far more often than by width, and a 48 px rail is the cheapest
//!   chrome there is.
//! - **Profiles are always visible.** They are the primary switch in an FTS
//!   plugin — they change what the editor *is* — so they are one click away
//!   rather than folded into a dropdown.
//! - **A profile is a badge, not a word.** "2A", "76", "SSL" is how the units
//!   are named on the gear anyway; the full name is the tooltip.
//!
//! Styling is inline rather than Tailwind: this renders under Blitz inside
//! DAW hosts, where an unresolved utility class means a collapsed layout
//! rather than an ugly one. Colours come from the architect-ui theme variables the
//! editors already set, with the plugin's accent passed in.

pub mod form;

pub use form::{EditorForm, EDITOR_FORMS};

use dioxus::prelude::*;

/// One entry in the rail.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellItem {
    /// Stable id — used for the `data-testid` (`rail-item-{id}`).
    pub id: String,
    /// The full name. Shown as the tooltip, never in the rail.
    pub label: String,
    /// The badge: two or three characters, the way the unit is named on its
    /// own front panel ("2A", "76", "SSL"). Derived from the label when not
    /// given, which is fine for one-word names and rarely right otherwise.
    pub badge: String,
    /// How many units this entry cycles through, and which one is showing.
    ///
    /// Clicking an active entry advances to the next unit inside it, and
    /// nothing on the rail said so: a family of three looked exactly like a
    /// family of one until you clicked it and something changed. The dots are
    /// the smallest honest answer — how many there are, and where you are.
    ///
    /// `None` (or a count of one) draws nothing, because a single-unit family
    /// has nothing to cycle.
    pub cycle: Option<(usize, usize)>,
}

impl ShellItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        let label = label.into();
        let badge = default_badge(&label);
        Self {
            id: id.into(),
            label,
            badge,
            cycle: None,
        }
    }

    /// Set the badge explicitly — the usual case for hardware names.
    pub fn with_badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = badge.into();
        self
    }

    /// Say how many units this entry cycles through, and which is active.
    pub fn with_cycle(mut self, count: usize, index: usize) -> Self {
        self.cycle = (count > 1).then_some((count, index.min(count.saturating_sub(1))));
        self
    }

    /// Build a list from `(id, label, badge)` triples, the usual call shape.
    pub fn list<'a>(entries: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>) -> Vec<Self> {
        entries
            .into_iter()
            .map(|(id, label, badge)| Self::new(id, label).with_badge(badge))
            .collect()
    }
}

/// First three characters of the first word, uppercased — the fallback badge.
fn default_badge(label: &str) -> String {
    label
        .split_whitespace()
        .next()
        .unwrap_or(label)
        .chars()
        .take(3)
        .collect::<String>()
        .to_uppercase()
}

/// Rail width in CSS px — the Task app's `w-12` ribbon.
pub const RAIL_W: f64 = 48.0;

/// The modifier-key type `on_select_mods` hands out, re-exported so shells
/// name it without a direct dioxus dependency.
pub use dioxus::prelude::Modifiers as RailModifiers;

/// The shell: identity + profile rail on the left, the plugin's surface on
/// the right.
///
/// `children` is the surface, and it is given a `position: relative` box that
/// fills what the rail leaves — so a surface can absolutely position floating
/// controls over itself without fighting the layout.
#[component]
pub fn PluginShell(
    /// "FTS Comp" — the tooltip on the brand chip.
    title: String,
    /// "Stereo Compressor" — the rest of that tooltip.
    subtitle: String,
    /// Two or three characters for the brand chip at the top of the rail.
    #[props(default = "FTS".to_string())]
    brand: String,
    /// The profile list. Empty hides the list entirely (plugins without
    /// profiles still get the shell for its identity and footer).
    #[props(default)]
    items: Vec<ShellItem>,
    #[props(default)] selected: usize,
    #[props(default)] on_select: Option<EventHandler<usize>>,
    /// Like `on_select`, but carrying the click's modifier keys — for shells
    /// whose rail distinguishes plain click (replace) from Shift-click (stack
    /// serially) and Ctrl+Shift-click (stack in parallel), per
    /// `fx.stack.add`. When set, `on_select` is not called.
    #[props(default)]
    on_select_mods: Option<EventHandler<(usize, Modifiers)>>,
    /// Accent for the selected item — the plugin's or the profile's colour.
    #[props(default = "#8aa4ff".to_string())]
    accent: String,
    /// Foot cluster: whatever the plugin wants one click away (a page
    /// toggle, bypass, presets). Separated from the list by a rule, as in
    /// the Task rail.
    #[props(default)]
    rail_footer: Option<Element>,
    #[props(default = RAIL_W)] rail_width: f64,
    children: Element,
) -> Element {
    rsx! {
        div {
            "data-testid": "plugin-shell",
            style: "position:absolute; inset:0; display:flex; align-items:stretch; \
                    overflow:hidden; background:var(--background); color:var(--foreground);",

            // ── Rail ─────────────────────────────────────────────────────
            div {
                "data-testid": "shell-rail",
                style: format!(
                    "width:{rail_width}px; flex:none; display:flex; flex-direction:column; \
                     align-items:center; gap:3px; padding:7px 0; overflow:hidden; \
                     border-right:1px solid var(--border, rgba(148,163,184,0.30)); \
                     background:var(--card, rgba(16,18,22,0.98));"
                ),

                // Brand chip. Doubles as the identity — there is no room for
                // a name, so the name is the tooltip.
                div {
                    "data-testid": "shell-brand",
                    title: "{title} — {subtitle}",
                    style: format!(
                        "width:32px; height:32px; border-radius:9px; flex:none; \
                         display:flex; align-items:center; justify-content:center; \
                         font-size:11px; font-weight:800; letter-spacing:0.02em; \
                         color:{accent}; background:color-mix(in oklab, {accent} 18%, transparent); \
                         border:1px solid color-mix(in oklab, {accent} 40%, transparent);"
                    ),
                    "{brand}"
                }

                if !items.is_empty() {
                    div {
                        style: "width:26px; height:1px; flex:none; margin:4px 0 3px; \
                                background:var(--border, rgba(148,163,184,0.30));",
                    }
                    div {
                        "data-testid": "shell-items",
                        style: "display:flex; flex-direction:column; align-items:center; \
                                gap:3px; min-height:0; overflow:hidden;",
                        for (index , item) in items.iter().enumerate() {
                            {
                                let active = index == selected;
                                let on_select = on_select;
                                let on_select_mods = on_select_mods;
                                rsx! {
                                    div {
                                        "data-testid": "rail-item-{item.id}",
                                        "data-active": "{active}",
                                        title: match item.cycle {
                                            Some((count, at)) => {
                                                format!("{} — {} of {count} (click to cycle)", item.label, at + 1)
                                            }
                                            None => item.label.clone(),
                                        },
                                        style: "display:flex; flex-direction:column; \
                                                align-items:center; gap:2px; flex:none; \
                                                cursor:pointer;",
                                        onclick: move |evt: MouseEvent| {
                                            if let Some(cb) = on_select_mods {
                                                cb.call((index, evt.modifiers()));
                                            } else if let Some(cb) = on_select {
                                                cb.call(index);
                                            }
                                        },
                                        div {
                                            style: format!(
                                                "width:32px; height:32px; border-radius:9px; \
                                                 display:flex; align-items:center; justify-content:center; \
                                                 font-size:10px; letter-spacing:0.02em; \
                                                 font-weight:{}; color:{}; background:{}; border:1px solid {};",
                                                if active { 800 } else { 600 },
                                                if active { "#0b0b0d" } else { "var(--muted-foreground, #8a8a92)" },
                                                if active { accent.as_str() } else { "transparent" },
                                                if active { accent.as_str() } else { "transparent" },
                                            ),
                                            "{item.badge}"
                                        }
                                        // How many units are stacked behind this
                                        // entry, and which one you are on.
                                        if let Some((count, at)) = item.cycle {
                                            div {
                                                "data-testid": "rail-dots-{item.id}",
                                                style: "display:flex; flex-direction:row; \
                                                        align-items:center; gap:2px; height:4px;",
                                                for dot in 0..count {
                                                    div {
                                                        key: "{dot}",
                                                        style: format!(
                                                            "width:3px; height:3px; border-radius:50%; \
                                                             background:{}; opacity:{};",
                                                            // Only the family you are in has a "you are
                                                            // here"; a lit dot on one you are not in
                                                            // would claim a position you do not hold.
                                                            if active && dot == at {
                                                                accent.as_str()
                                                            } else {
                                                                "var(--muted-foreground, #8a8a92)"
                                                            },
                                                            if active && dot == at { "1" } else { "0.40" },
                                                        ),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Pushes the foot cluster down without needing a height on
                // anything.
                div { style: "flex:1; min-height:0;" }

                if let Some(footer) = rail_footer {
                    div {
                        "data-testid": "shell-rail-footer",
                        style: "display:flex; flex-direction:column; align-items:center; \
                                gap:3px; flex:none; padding-top:6px; \
                                border-top:1px solid var(--border, rgba(148,163,184,0.30)); \
                                width:26px;",
                        {footer}
                    }
                }
            }

            // ── Surface ──────────────────────────────────────────────────
            div {
                "data-testid": "shell-surface",
                style: "flex:1; min-width:0; position:relative; display:flex; \
                        flex-direction:column; overflow:hidden;",
                {children}
            }
        }
    }
}

/// A rail-sized button for the foot cluster — same 32 px pill as a profile
/// badge, so a page toggle or a bypass does not look like a different kit.
#[component]
pub fn RailButton(
    label: String,
    /// Tooltip; falls back to the label.
    #[props(default)]
    title: Option<String>,
    #[props(default = false)] active: bool,
    #[props(default = "#8aa4ff".to_string())] accent: String,
    #[props(default)] testid: Option<String>,
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            "data-testid": testid.unwrap_or_else(|| "rail-button".to_string()),
            "data-active": "{active}",
            title: title.unwrap_or_else(|| label.clone()),
            style: format!(
                "width:32px; height:32px; border-radius:9px; flex:none; \
                 display:flex; align-items:center; justify-content:center; \
                 cursor:pointer; font-size:10px; font-weight:{}; letter-spacing:0.02em; \
                 color:{}; background:{}; border:1px solid {};",
                if active { 800 } else { 600 },
                if active { "#0b0b0d" } else { "var(--muted-foreground, #8a8a92)" },
                if active { accent.as_str() } else { "transparent" },
                if active {
                    accent.as_str()
                } else {
                    "var(--border, rgba(148,163,184,0.26))"
                },
            ),
            onclick: move |_| on_click.call(()),
            "{label}"
        }
    }
}

/// A floating control cluster over a full-window surface.
///
/// The FTS surfaces are moving to "the graph *is* the editor", with controls
/// floating on top of it instead of taking a row underneath. Floating is also
/// the robust choice under Blitz: an absolutely positioned overlay cannot be
/// squeezed to 0 px by a flex parent the way a sibling row can, so controls
/// stay reachable at any window size.
#[component]
pub fn FloatingPanel(
    /// Where it sits, as raw CSS inset properties — e.g.
    /// `"left:12px; right:12px; bottom:12px;"`.
    position: String,
    #[props(default)] testid: Option<String>,
    /// Lay the children out in a row (the default) or a column.
    #[props(default = false)]
    column: bool,
    /// `justify-content` for the children — "center" for a bar stretched
    /// across the surface, the default for one that hugs its corner.
    #[props(default = "flex-start".to_string())]
    justify: String,
    /// Let the children wrap onto more rows. A bottom-anchored bar grows
    /// upward over the surface, which is the right failure mode for a page
    /// with more controls than fit across.
    #[props(default = false)]
    wrap: bool,
    #[props(default = 10.0)] gap: f64,
    children: Element,
) -> Element {
    rsx! {
        div {
            "data-testid": testid.unwrap_or_else(|| "floating-panel".to_string()),
            style: format!(
                "position:absolute; {position} display:flex; \
                 flex-direction:{}; align-items:center; justify-content:{justify}; \
                 flex-wrap:{}; gap:{gap}px; \
                 padding:8px 12px; border-radius:10px; \
                 background:color-mix(in oklab, var(--card, #101216) 82%, transparent); \
                 border:1px solid var(--border, rgba(148,163,184,0.26)); \
                 box-shadow:0 6px 22px rgba(0,0,0,0.45);",
                if column { "column" } else { "row" },
                if wrap { "wrap" } else { "nowrap" },
            ),
            {children}
        }
    }
}

#[cfg(test)]
mod cycle_tests {
    use super::*;

    /// A family with one unit has nothing to cycle, so it draws no dots —
    /// otherwise every single-entry rail would grow a dot that means nothing.
    #[test]
    fn one_unit_draws_no_dots() {
        assert_eq!(ShellItem::new("main", "Main").with_cycle(1, 0).cycle, None);
        assert_eq!(ShellItem::new("main", "Main").with_cycle(0, 0).cycle, None);
    }

    /// …and a family of several says how many and where you are.
    #[test]
    fn several_units_say_how_many_and_which() {
        let item = ShellItem::new("opto", "Opto").with_cycle(3, 1);
        assert_eq!(item.cycle, Some((3, 1)));
    }

    /// An index past the end is clamped rather than dropped: the caller is
    /// reporting a position, and a position that cannot be drawn is worse
    /// than one that is drawn at the end.
    #[test]
    fn an_out_of_range_position_lands_on_the_last_dot() {
        assert_eq!(
            ShellItem::new("x", "X").with_cycle(3, 9).cycle,
            Some((3, 2))
        );
    }
}
