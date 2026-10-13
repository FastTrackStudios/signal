//! The FX insert slots above a mixer strip, and the embedded-GUI seam.
//!
//! REAPER's MCP grows an FX list above the strip, and since REAPER 6 a
//! plugin can *embed* a small GUI right in its slot. Ours works the same
//! way with one structural difference: the FTS plugins' GUIs are Dioxus
//! components already, so "embedding" is composition, not window
//! re-parenting.
//!
//! # The seam
//!
//! `daw-ui` must not depend on the plugin crates (their DSP, their
//! GUIs) — the mixer would drag the whole FX suite into every panel
//! build. So embedding is a **registry in context**:
//! [`EmbeddedFxGuis`] maps a plugin-name fragment to a plain
//! `fn(EmbeddedFx) -> Element`, the app that owns the plugin GUI crates
//! registers its providers at startup, and a slot whose plugin matches a
//! provider can expand and render it. No provider, no expansion — the
//! slot is a name and a bypass lamp, which is what REAPER shows for a
//! plugin that does not embed.
//!
//! Parameter traffic is the provider's business: a real plugin GUI talks
//! to its FX over whatever wire it already has (the fx params RPC, the
//! plugin's own vox link). This module only gives it the cell.

use daw_proto::Fx;

use crate::prelude::*;

/// What an embedded GUI is given: the FX it renders and the cell it must
/// fill. Everything else it finds itself.
#[derive(Clone, PartialEq)]
pub struct EmbeddedFx {
    pub fx: Fx,
    pub width: f32,
    pub height: f32,
}

/// A provider: draws one plugin's embedded GUI.
pub type EmbeddedFxRenderer = fn(EmbeddedFx) -> Element;

/// The registry of embedded FX GUIs, carried in context.
///
/// Keys are matched as case-blind substrings of the FX's `plugin_name`,
/// so `"FTS EQ"` catches `"VST3: FTS EQ (FastTrackStudio)"` however the
/// host decorates it. First match wins; register the specific before the
/// general.
#[derive(Clone, Copy, Default)]
pub struct EmbeddedFxGuis {
    providers: Signal<Vec<(String, EmbeddedFxRenderer)>>,
}

impl EmbeddedFxGuis {
    /// Register a provider. Called by the app that owns the GUI crate.
    pub fn register(&mut self, plugin: impl Into<String>, renderer: EmbeddedFxRenderer) {
        self.providers.write().push((plugin.into(), renderer));
    }

    /// The provider for a plugin, if one is registered.
    pub fn provider_for(&self, plugin_name: &str) -> Option<EmbeddedFxRenderer> {
        let name = plugin_name.to_ascii_lowercase();
        self.providers
            .read()
            .iter()
            .find(|(key, _)| name.contains(&key.to_ascii_lowercase()))
            .map(|(_, r)| *r)
    }
}

/// The registry from context, created empty on first use so a test or a
/// window without providers still renders plain slots.
pub fn use_embedded_fx_guis() -> EmbeddedFxGuis {
    use_hook(|| match try_consume_context::<EmbeddedFxGuis>() {
        Some(registry) => registry,
        None => provide_context(EmbeddedFxGuis::default()),
    })
}

/// The height an expanded slot gives its embedded GUI.
pub const EMBED_H: f32 = 96.0;
/// One slot row.
const SLOT_H: f32 = 15.0;

/// The insert stack above one strip.
///
/// Rows top to bottom in chain order, each a name and a bypass lamp; the
/// expanded slot (clicked, or seeded by the `expanded` prop) grows
/// [`EMBED_H`] and renders its plugin's embedded GUI when the registry
/// has one.
#[component]
pub fn FxSlotStack(
    fx: Vec<Fx>,
    width: f32,
    /// The initially expanded FX, by guid.
    #[props(default)]
    expanded: Option<String>,
) -> Element {
    let t = daw_theme::Theme::default();
    let registry = use_embedded_fx_guis();
    let mut expanded = use_signal(move || expanded);

    let bg = t.chrome.surface_sunken.shade(-0.05).css();
    let rule = t.chrome.surface_sunken.shade(-0.22).css();
    let ink = t.chrome.text.shade(0.1).css();
    let dim = t.chrome.text_faint.css();
    let on = t.chrome.accent.css();
    let off = t.signal.meter_danger.css();
    let well = t.chrome.surface_sunken.shade(-0.12).css();

    rsx! {
        div {
            style: "width:{width}px; display:flex; flex-direction:column; \
                    background:{bg}; border-bottom:1px solid {rule}; \
                    overflow:hidden;",
            for f in fx.iter() {
                {
                    let guid = f.guid.clone();
                    let is_open = expanded.read().as_deref() == Some(f.guid.as_str());
                    let provider = registry.provider_for(&f.plugin_name);
                    let can_embed = provider.is_some();
                    let lamp = if f.enabled { on.clone() } else { off.clone() };
                    let name_ink = if f.enabled { ink.clone() } else { dim.clone() };
                    rsx! {
                        div {
                            key: "{f.guid}",
                            // The row: lamp, name.
                            div {
                                style: "height:{SLOT_H}px; display:flex; \
                                        align-items:center; gap:4px; padding:0 5px; \
                                        border-bottom:1px solid {rule}; \
                                        cursor:pointer;",
                                onclick: move |_| {
                                    let mut open = expanded.write();
                                    *open = if open.as_deref() == Some(guid.as_str()) {
                                        None
                                    } else {
                                        Some(guid.clone())
                                    };
                                },
                                div {
                                    style: "flex:0 0 auto; width:5px; height:5px; \
                                            border-radius:50%; background:{lamp};",
                                }
                                div {
                                    style: "flex:1 1 auto; min-width:0; font-size:8px; \
                                            color:{name_ink}; white-space:nowrap; \
                                            overflow:hidden; text-overflow:ellipsis; \
                                            font-family:Fira Sans, DejaVu Sans, sans-serif;",
                                    "{f.name}"
                                }
                                if can_embed {
                                    // The affordance that a GUI is behind
                                    // the row.
                                    div {
                                        style: "flex:0 0 auto; font-size:7px; color:{dim};",
                                        if is_open { "▾" } else { "▸" }
                                    }
                                }
                            }
                            // The embedded GUI's cell.
                            if is_open {
                                match provider {
                                    Some(render) => {
                                        render(EmbeddedFx {
                                            fx: f.clone(),
                                            width,
                                            height: EMBED_H,
                                        })
                                    }
                                    None => rsx! {
                                        div {
                                            style: "height:{EMBED_H * 0.4}px; display:flex; \
                                                    align-items:center; justify-content:center; \
                                                    background:{well}; font-size:8px; color:{dim}; \
                                                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
                                            "no embedded GUI"
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
