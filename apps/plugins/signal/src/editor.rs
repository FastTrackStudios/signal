//! The Signal editor: the kit's mixer, for a DAW with no view of its own.
//!
//! Every port of this instance's session ([`crate::strip`], from
//! `SIGNAL_SESSION`), one strip each, its phases and their blocks as
//! inserts. A block's or a phase's bypass is switched from here on whichever
//! port it is (`SIGNAL_SET` on that instance); the parameters show their
//! value in the block's own words, and are set from the host's own
//! parameter view. Refreshed a few times a second.
//!
//! Blitz: inline styles only, no `position: fixed`, `overflow-y: scroll`.

use std::sync::Arc;

use plugin_core::prelude::*;

use crate::plugin::Shared;
use crate::strip::{Insert, PhaseStrip, Session, Strip};

/// What the editor reads: its own instance (and through it, the session).
pub struct EditorState {
    pub shared: Arc<Shared>,
}

/// An instance, as a component's prop: the same one when the same pointer.
#[derive(Clone)]
pub struct Own(pub Arc<Shared>);

impl PartialEq for Own {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The editor window's first size: a row of strips.
pub const SIZE: (u32, u32) = (980, 620);

const BG: &str = "#141418";
const PANEL: &str = "#1d1d23";
const LINE: &str = "#2a2a32";
const TEXT: &str = "#e8e8ec";
const DIM: &str = "#8b8b97";
const ON: &str = "#4ade80";
const OFF: &str = "#f87171";

fn phase_tint(id: &str) -> &'static str {
    // Studio's phase inks (session's `mix_phases`).
    match id {
        "rescue" => "#ef4444",
        "tone" => "#facc15",
        "polish" => "#06b6d4",
        "relational" => "#3b82f6",
        "depth" => "#8b5cf6",
        _ => "#8b8b97",
    }
}

/// Switch a block's or a phase's bypass on the instance that runs `port`.
fn set_bypass(own: &Own, port: Option<&str>, args: serde_json::Value) {
    let target = crate::ext::session_of_instance(&own.0)
        .into_iter()
        .find(|s| {
            crate::ext::run(s, "SIGNAL_CHAIN", &serde_json::Value::Null, false)
                .ok()
                .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                .is_some_and(|c| c["port"].as_str() == port)
        });
    if let Some(s) = target {
        if let Err(e) = crate::ext::run(&s, "SIGNAL_SET", &args, false) {
            tracing::warn!(error = %e, "editor: SIGNAL_SET refused");
        }
    }
}

/// Editor root. Props-less, as the plugin surface requires.
#[component]
pub fn App() -> Element {
    let shared = use_context::<SharedState>();
    let Some(state) = shared.get::<EditorState>() else {
        return rsx! { div { style: "padding:16px;color:{TEXT};background:{BG};", "Signal editor state missing." } };
    };
    // Once a second: the session's chains are polled (each parses its state).
    let mut tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let update = plugin_core::nice_plug_dioxus::dioxus_vdom::schedule_update();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                update();
            }
        });
    });
    tick += 1;
    let frame = *tick.read();
    let own = Own(state.shared.clone());
    let session = crate::ext::run(&own.0, "SIGNAL_SESSION", &serde_json::Value::Null, false)
        .map(|j| Session::parse(&j))
        .unwrap_or_default();
    let title = session.session.clone().unwrap_or_else(|| "Signal".into());
    let count = session.ports.len();
    rsx! {
        div {
            "data-frame": "{frame}",
            style: "display:flex;flex-direction:column;height:100vh;background:{BG};color:{TEXT};font:12px/1.35 system-ui,sans-serif;",
            div {
                style: "display:flex;align-items:center;gap:10px;padding:8px 12px;border-bottom:1px solid {LINE};",
                span { style: "font-size:11px;letter-spacing:0.08em;text-transform:uppercase;color:{DIM};", "Kit" }
                span { style: "font-size:14px;font-weight:600;", "{title}" }
                span { style: "color:{DIM};", "{count} tracks" }
            }
            div {
                style: "flex:1;min-height:0;overflow-y:scroll;display:flex;flex-wrap:wrap;gap:8px;padding:8px;align-content:flex-start;",
                for strip in session.ports.iter() {
                    StripView { key: "{strip.port:?}", strip: strip.clone(), own: own.clone() }
                }
            }
        }
    }
}

#[component]
fn StripView(strip: Strip, own: Own) -> Element {
    let name = strip.name().to_owned();
    let path = strip.port.clone().unwrap_or_default();
    rsx! {
        div {
            style: "width:220px;background:{PANEL};border:1px solid {LINE};border-radius:6px;display:flex;flex-direction:column;",
            div { style: "padding:6px 8px;border-bottom:1px solid {LINE};",
                div { style: "font-weight:600;", "{name}" }
                div { style: "font-size:10px;color:{DIM};", "{path}" }
            }
            for phase in strip.phases.iter() {
                PhaseView { key: "{phase.id}", phase: phase.clone(), port: strip.port.clone(), own: own.clone() }
            }
        }
    }
}

#[component]
fn PhaseView(phase: PhaseStrip, port: Option<String>, own: Own) -> Element {
    let tint = phase_tint(&phase.id);
    let state = if phase.bypass { OFF } else { ON };
    let makeup = format!("{:+.1} dB", phase.makeup_db);
    let (own_p, port_p, id_p, bypass_p) =
        (own.clone(), port.clone(), phase.id.clone(), phase.bypass);
    rsx! {
        div { style: "border-top:1px solid {LINE};padding:4px 6px;",
            div { style: "display:flex;align-items:center;gap:6px;",
                span { style: "width:3px;height:12px;background:{tint};" }
                span { style: "flex:1;font-size:11px;text-transform:uppercase;letter-spacing:0.06em;color:{DIM};", "{phase.name}" }
                span { style: "font-size:10px;color:{DIM};", "{makeup}" }
                button {
                    style: "border:0;border-radius:3px;padding:1px 6px;background:{LINE};color:{state};font-size:10px;",
                    onclick: move |_| set_bypass(&own_p, port_p.as_deref(), serde_json::json!({ "phase": id_p, "bypass": !bypass_p })),
                    if phase.bypass { "off" } else { "on" }
                }
            }
            for block in phase.blocks.iter() {
                InsertView { key: "{block.id}", block: block.clone(), port: port.clone(), own: own.clone() }
            }
        }
    }
}

#[component]
fn InsertView(block: Insert, port: Option<String>, own: Own) -> Element {
    let state = if block.bypass { OFF } else { ON };
    let via = block.key.as_ref().map(|k| {
        format!(
            "key: {} ({})",
            k.from.rsplit('/').next().unwrap_or(&k.from),
            k.via
        )
    });
    let shown: Vec<_> = block
        .params
        .iter()
        .filter(|p| p.clap_id.is_some())
        .take(6)
        .cloned()
        .collect();
    let (id, bypass) = (block.id.clone(), block.bypass);
    let unavailable = block.status != "ok";
    rsx! {
        div { style: "margin:4px 0 2px 9px;padding:3px 5px;background:{BG};border-radius:4px;",
            div { style: "display:flex;align-items:center;gap:6px;",
                span { style: "flex:1;font-weight:600;", "{block.name}" }
                if unavailable { span { style: "font-size:10px;color:{OFF};", "{block.status}" } }
                button {
                    style: "border:0;border-radius:3px;padding:1px 6px;background:{LINE};color:{state};font-size:10px;",
                    onclick: move |_| set_bypass(&own, port.as_deref(), serde_json::json!({ "block": id, "bypass": !bypass })),
                    if block.bypass { "off" } else { "on" }
                }
            }
            if let Some(via) = via { div { style: "font-size:10px;color:{DIM};", "{via}" } }
            for p in shown.iter() {
                div { style: "display:flex;font-size:10px;",
                    span { style: "flex:1;color:{DIM};", "{p.name}" }
                    span { "{p.text}" }
                }
            }
        }
    }
}
