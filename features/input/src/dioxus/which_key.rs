//! App-agnostic Dioxus which-key overlay.
//!
//! A live "which-key" popup: given the current key `sequence` and the set of
//! continuation `entries` available at that prefix, it renders a themed card
//! listing each next key, its label, and whether it opens a deeper branch.
//!
//! This component is deliberately host-neutral — it renders identically in a
//! plain Dioxus app, a plugin editor, or an embedded native surface. The
//! consuming app supplies the [`WhichKeyState`] via component context (e.g.
//! `use_context_provider`, or the `with_context` on a native overlay
//! builder). Colours/borders come from the `architect_ui` theme + Tailwind classes;
//! the size-sensitive properties are inline-styled and multiplied by
//! [`WhichKeyState::scale`] so the popup scales across devices without a CSS
//! rebuild. The host is responsible for injecting a Tailwind stylesheet if it
//! wants the class-based colours (this component only injects the theme).

use architect_ui::prelude::*;
use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// State for the which-key overlay, injected as component context.
#[derive(Clone)]
pub struct WhichKeyState {
    pub entries: Vec<OverlayEntry>,
    pub sequence: String,
    pub header_label: String,
    /// Multiplicative size factor derived from the host arrange-view
    /// dimensions. 1.0 = design size; smaller on cramped laptop screens,
    /// larger on big external monitors. Applied as inline styles on the
    /// popup so the same component renders proportionally across devices.
    pub scale: f32,
}

impl Default for WhichKeyState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            sequence: String::new(),
            header_label: String::new(),
            scale: 1.0,
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct OverlayEntry {
    pub key: String,
    pub label: String,
    pub is_branch: bool,
    pub available: bool,
}

// ---------------------------------------------------------------------------
// Root component
// ---------------------------------------------------------------------------

#[component]
pub fn WhichKeyOverlay() -> Element {
    let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));
    let state = use_context::<WhichKeyState>();
    let scale = state.scale.max(0.5);

    let header_text = if state.header_label.is_empty() {
        state.sequence.clone()
    } else {
        state.header_label.clone()
    };

    // Inline styles parameterised by `scale`. Tailwind still handles
    // colours / borders / radii / shadow — only the size-sensitive
    // properties (paddings, gaps, font-sizes) come through here so the
    // overlay scales proportionally without a CSS rebuild.
    let card_style = format!(
        "border-radius: {radius}px; overflow: hidden; display: flex; \
         flex-direction: column; width: max-content; height: max-content;",
        radius = px(6.0, scale)
    );
    let header_style = format!(
        "padding: {pv}px {ph}px; border-bottom-width: 1px;",
        pv = px(8.0, scale),
        ph = px(12.0, scale)
    );
    let header_text_style = format!(
        "font-size: {fs}px; font-weight: 500; margin: 0;",
        fs = px(12.0, scale)
    );
    let content_style = format!(
        "padding: {pv}px {ph}px; display: flex; flex-direction: column; \
         gap: {g}px;",
        pv = px(8.0, scale),
        ph = px(12.0, scale),
        g = px(4.0, scale)
    );

    // Outer wrapper is explicitly width:max-content so neither the
    // ThemeProvider nor the Blitz root viewport stretches the card.
    let outer_style = "display: inline-block; width: max-content; height: max-content; \
                       background: transparent;";

    rsx! {
        ThemeProvider {
            state: theme_state,
            class: "text-foreground",
            div {
                style: "{outer_style}",
                div {
                    class: "border border-border/80 bg-card/95 shadow-xl",
                    style: "{card_style}",
                    div {
                        class: "border-b border-border/70 text-left text-foreground",
                        style: "{header_style}",
                        p {
                            class: "text-foreground whitespace-nowrap",
                            style: "{header_text_style}",
                            "{header_text}"
                        }
                    }
                    div {
                        style: "{content_style}",
                        for entry in state.entries.iter() {
                            WhichKeyEntry { entry: entry.clone(), scale }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Entry row component
// ---------------------------------------------------------------------------

#[component]
fn WhichKeyEntry(entry: OverlayEntry, scale: f32) -> Element {
    let opacity_class = if entry.available { "" } else { "opacity-45" };
    let label_color = if entry.available {
        "text-zinc-100"
    } else {
        "text-zinc-500"
    };

    let row_style = format!(
        "display: flex; align-items: center; gap: {gap}px; \
         padding: 0 {px_h}px; height: {h}px; font-size: {fs}px; \
         white-space: nowrap; border-radius: 2px;",
        gap = px(6.0, scale),
        px_h = px(4.0, scale),
        h = px(18.0, scale),
        fs = px(11.0, scale),
    );
    let kbd_style = format!(
        "display: inline-flex; align-items: center; justify-content: center; \
         height: {h}px; min-width: {mw}px; padding: 0 {px_h}px; \
         font-family: ui-monospace,monospace; font-size: {fs}px; \
         border: 1px solid rgba(255,255,255,0.15); border-radius: 3px; \
         background: rgba(255,255,255,0.06);",
        h = px(14.0, scale),
        mw = px(16.0, scale),
        px_h = px(4.0, scale),
        fs = px(10.0, scale),
    );
    let badge_style = format!(
        "display: inline-flex; align-items: center; justify-content: center; \
         height: {h}px; padding: 0 {px_h}px; font-size: {fs}px; \
         border-radius: 4px; background: rgba(255,255,255,0.1);",
        h = px(12.0, scale),
        px_h = px(3.0, scale),
        fs = px(9.0, scale),
    );

    rsx! {
        div {
            class: "{opacity_class}",
            style: "{row_style}",
            span {
                class: "text-foreground",
                style: "{kbd_style}",
                "{entry.key}"
            }
            span { class: "text-muted-foreground/50", "\u{203A}" }
            if entry.is_branch {
                span { class: "text-foreground", style: "{badge_style}", "+" }
                span { class: "{label_color} font-medium", "{entry.label}" }
            } else {
                span { class: "{label_color}", "{entry.label}" }
            }
        }
    }
}

/// Multiply a design pixel value by the runtime scale, returning an
/// integer pixel count. Centralised so callers don't repeat the
/// `(value * scale).round() as i32` pattern.
fn px(value: f64, scale: f32) -> i32 {
    ((value as f32) * scale).round() as i32
}
