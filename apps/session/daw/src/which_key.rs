//! The which-key popup: after the first key of a sequence (`z`, "Zoom"),
//! what can follow it and what each does, from the keybind profile's own
//! labels ([`crate::keys::Keys::which_key`]).
//!
//! The widget owns the keyboard, so it writes what to show into a
//! [`Shared`] cell as keys come in; the Arrangement panel reads it once a
//! frame into a signal and renders [`Panel`]. Inline styles only (Blitz
//! does not load stylesheets reliably), and one DOM shape whether it is
//! showing or not: nodes that come and go with the props are what blitz
//! walks a stale path through (the `node_at_path` "invalid key" panic),
//! so an absent popup is `display:none`, not an absent element.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;

/// What the popup shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WhichKey {
    /// The group's name: "Zoom".
    pub title: String,
    /// The keys typed so far: "z".
    pub typed: String,
    pub entries: Vec<Entry>,
}

/// One row: a key and what it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub label: String,
    /// It opens a further group rather than doing something.
    pub group: bool,
    /// This window does what it is bound to. A binding it does not do yet
    /// is still listed, dimmed, so the tree reads as the profile has it.
    pub available: bool,
}

/// Widget to panel: what to show now, `None` for nothing.
pub type Shared = Rc<RefCell<Option<WhichKey>>>;

/// The popup, bottom-right over the lanes.
#[component]
pub fn Panel(showing: Option<WhichKey>, colors: daw_ui::studio::lanes::Colors) -> Element {
    let visible = showing.is_some();
    let which = showing.unwrap_or_default();
    let display = if visible { "block" } else { "none" };
    let heading = if which.title.is_empty() {
        which.typed.clone()
    } else {
        format!("{}  {}", which.typed, which.title)
    };
    let rows: Vec<(String, String, String)> = which
        .entries
        .into_iter()
        .map(|entry| {
            let label = if entry.group {
                format!("+{}", entry.label)
            } else {
                entry.label
            };
            let ink = if entry.available {
                colors.text.clone()
            } else {
                colors.faint.clone()
            };
            (entry.key, label, ink)
        })
        .collect();
    let bottom = crate::shell::BOTTOM_H + 16.0;
    rsx! {
        div {
            "data-testid": "which-key",
            // The window's corner, not the arrangement's: the popup sits
            // in the same place whichever pane the keys are typed over,
            // and clear of the bottom bar.
            style: "display:{display}; position:fixed; right:22px; bottom:{bottom}px; z-index:50; \
                    min-width:240px; padding:6px 0; border-radius:6px; \
                    border:1px solid {colors.divider}; background:{colors.surface}; \
                    color:{colors.text}; font-size:12px; \
                    box-shadow:0 6px 24px rgba(0,0,0,0.45);",
            div {
                style: "padding:2px 12px 6px; margin-bottom:4px; \
                        border-bottom:1px solid {colors.divider}; \
                        color:{colors.accent}; font-weight:600;",
                "{heading}"
            }
            for (key, label, ink) in rows.into_iter() {
                div {
                    key: "{key}",
                    style: "display:flex; gap:10px; padding:2px 12px;",
                    span {
                        style: "min-width:26px; font-weight:600; color:{colors.accent};",
                        "{key}"
                    }
                    span { style: "color:{ink};", "{label}" }
                }
            }
        }
    }
}
