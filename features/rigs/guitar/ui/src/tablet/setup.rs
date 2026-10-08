//! Setup — the prototype's `views/Setup.tsx` on the rig: choose a guitar,
//! choose a rig, and it is set.
//!
//!   Guitar  its photo, its pickups, and its tone — the input trim (with a
//!           level match), the four gate thresholds (Noisy input lifts Off
//!           to Subtle and Subtle to Default) and its input EQ. The tone is
//!           the guitar's default on every rig; a rig can override any part
//!           of it, marked with the override icon and saved back to the
//!           guitar or discarded.
//!   Audio   the rig's interface: device, rate and buffer (and the round
//!           trip they make), the guitar's input, the house and phones.
//!   MIDI    the rig's controller: its switches, channel, clock, program
//!           changes.
//!
//! The setup is the rig's (`setup` / `save_setup`): an edit changes the copy
//! here and sends it back whole; a drag sends it when the finger lifts.
//! Levels are the rig's own meters.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{AudioRigEntry, ControllerEntry, GuitarEntry, GuitarTone, Pickup, SetupModel};
use signal_widgets::drag_bus::{DragBus, DragEvent};
use signal_widgets::PopupHost;

use super::marks::OverrideIcon;
use super::menu::{open_menu, Item, MoreButton, Picked};
use super::tokens::*;
use crate::state::RigViewState;

/// A rig's override of the guitar, in the rig's colour.
const RIG: &str = "#38bdf8";
const WARN: &str = "#eab308";
const THUMB: &str = "#f4f4f5";
const GATE_NAMES: [&str; 4] = ["Subtle", "Default", "Tight", "Ultra"];
const GATE_MIN: f64 = -96.0;
const GATE_MAX: f64 = -24.0;

/// What an interface offers — the choices its settings are made from.
struct Interface {
    name: &'static str,
    inputs: &'static [(&'static str, &'static str)],
    outputs: &'static [&'static str],
    phones: &'static [&'static str],
    rates: &'static [u32],
    buffers: &'static [u32],
    converter_ms: f64,
}

const INTERFACES: &[Interface] = &[
    Interface {
        name: "Arturia MiniFuse 4",
        inputs: &[("Input 1", "Inst"), ("Input 2", "Inst"), ("Input 3", "Line"), ("Input 4", "Line")],
        outputs: &["Outputs 1–2", "Outputs 3–4"],
        phones: &["Phones 1", "Phones 2"],
        rates: &[44_100, 48_000, 88_200, 96_000, 192_000],
        buffers: &[32, 64, 128, 256, 512, 1024],
        converter_ms: 1.9,
    },
    Interface {
        name: "Focusrite Scarlett 2i2",
        inputs: &[("Input 1", "Inst"), ("Input 2", "Inst")],
        outputs: &["Outputs 1–2"],
        phones: &["Phones"],
        rates: &[44_100, 48_000, 88_200, 96_000, 176_400, 192_000],
        buffers: &[16, 32, 64, 128, 256, 512, 1024],
        converter_ms: 1.4,
    },
    Interface {
        name: "Built-in",
        inputs: &[("Microphone", "Mic/Line")],
        outputs: &["Speakers"],
        phones: &["Headphones"],
        rates: &[44_100, 48_000],
        buffers: &[128, 256, 512, 1024],
        converter_ms: 4.0,
    },
];

/// MIDI controllers, how each connects, and its switches.
const MIDI_DEVICES: &[(&str, &str, u32)] = &[("XSonic AIRSTEP", "Bluetooth", 5), ("Morningstar MC6", "USB", 6)];

/// What any other interface offers, as far as the rig asks of it.
const GENERIC: Interface = Interface {
    name: "",
    inputs: &[("Input 1", "Inst"), ("Input 2", "Inst")],
    outputs: &["Outputs 1–2"],
    phones: &["Phones"],
    rates: &[44_100, 48_000, 96_000],
    buffers: &[32, 64, 128, 256, 512, 1024],
    converter_ms: 2.0,
};

fn interface_of(r: &AudioRigEntry) -> &'static Interface {
    INTERFACES.iter().find(|i| i.name == r.device).unwrap_or(&GENERIC)
}

fn latency_ms(r: &AudioRigEntry) -> f64 {
    let rate = f64::from(r.rate.max(1));
    2.0 * f64::from(r.buffer) * 1000.0 / rate + interface_of(r).converter_ms
}

fn khz(rate: u32) -> String {
    let k = f64::from(rate) / 1000.0;
    if (k - k.round()).abs() < 0.05 { format!("{} kHz", k.round()) } else { format!("{k:.1} kHz") }
}

const LIVE_WASH: &str = "rgba(34,197,94,0.08)";
const LEVEL_OPEN: &str = "rgba(34,197,94,0.55)";
const KEY_DOWN: &str = "#3a3a42";
const GUITAR: &str = "Guitar";
const END: &str = "flex-end";
const START: &str = "flex-start";

fn left_rule(on: bool) -> String {
    if on { format!("border-left: 1px solid {RULE};") } else { String::new() }
}
fn top_rule(on: bool) -> String {
    if on { format!("border-top: 1px solid {RULE};") } else { String::new() }
}
fn outline(on: bool) -> String {
    if on { format!("1px solid {RULE_STRONG}") } else { "none".to_string() }
}
fn tick(d: i32) -> String {
    if d == 0 { "0".to_string() } else { format!("−{}", -d) }
}
fn link_label(link: &str) -> &'static str {
    if link == "Bluetooth" { "Bluetooth MIDI" } else { "USB" }
}
fn running_label(on: bool) -> &'static str {
    if on { "Connected" } else { "Not connected" }
}

/// The device choices: the known interfaces, the devices the system has,
/// and the one chosen (whatever it is).
fn device_options(present: &[String], chosen: &str) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut add = |name: &str, detail: &str| {
        if !name.is_empty() && !out.iter().any(|o| o.0 == name) {
            out.push((name.to_string(), name.to_string(), detail.to_string()));
        }
    };
    for p in present {
        add(p, "Connected");
    }
    for i in INTERFACES {
        add(i.name, "");
    }
    add(chosen, "");
    out
}

fn signed(v: f64) -> String {
    if v > 0.0 { format!("+{}", trim0(v)) } else { trim0(v) }
}

/// A number without a trailing `.0`.
fn trim0(v: f64) -> String {
    if (v - v.round()).abs() < 0.001 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

fn tone_name(part: &str) -> &'static str {
    match part {
        "trim" => "Trim",
        "gates" => "Gate",
        "noisy" => "Noisy input",
        _ => "EQ",
    }
}

fn gates_above(floor: f64) -> Vec<f32> {
    [5.0, 10.0, 16.0, 24.0].iter().map(|d| (floor + d) as f32).collect()
}

// ── The setup, held here and sent back ─────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SetupTab {
    Guitar,
    Audio,
    Midi,
}

impl SetupTab {
    pub fn from_id(id: &str) -> Self {
        match id {
            "audio" => Self::Audio,
            "midi" => Self::Midi,
            _ => Self::Guitar,
        }
    }
}

/// The setup copy and the way to change it.
#[derive(Clone, Copy)]
struct Setup {
    model: Signal<SetupModel>,
    rig: Signal<Option<RigClient>>,
    /// Photos fetched from the rig, by file name, as data URLs ("" when
    /// the rig has none).
    photos: Signal<std::collections::HashMap<String, String>>,
}

impl Setup {
    /// Change the copy and send it.
    fn edit(self, f: impl FnOnce(&mut SetupModel)) {
        let mut m = self.model;
        f(&mut m.write());
        self.save();
    }
    /// Change the copy only (a drag; [`save`](Self::save) when it lifts).
    fn tune(self, f: impl FnOnce(&mut SetupModel)) {
        let mut m = self.model;
        f(&mut m.write());
    }
    fn save(self) {
        let model = self.model.peek().clone();
        if let Some(r) = self.rig.peek().clone() {
            let _ = dioxus_core::spawn_forever(async move {
                let _ = r.save_setup(model).await;
            });
        }
    }
}

fn guitar(m: &SetupModel) -> GuitarEntry {
    m.guitars.get(m.guitar_index as usize).or(m.guitars.first()).cloned().unwrap_or_default()
}
fn audio_rig(m: &SetupModel) -> AudioRigEntry {
    m.rigs.get(m.rig_index as usize).or(m.rigs.first()).cloned().unwrap_or_default()
}
fn controller(m: &SetupModel) -> ControllerEntry {
    m.controllers.get(m.controller_index as usize).or(m.controllers.first()).cloned().unwrap_or_default()
}

/// Where a tone change lands: the guitar's default, or this rig only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scope {
    Guitar,
    Rig,
}

/// Change `part` of the playing guitar's tone in `scope`.
fn toning(m: &mut SetupModel, scope: Scope, part: &str, f: impl FnOnce(&mut GuitarTone)) {
    let rig = audio_rig(m).id;
    let k = m.guitar_index as usize;
    let Some(g) = m.guitars.get_mut(k) else { return };
    match scope {
        // A default changed under an override stays under it.
        Scope::Guitar => f(&mut g.tone),
        Scope::Rig => {
            let mut t = g.tone_on(&rig);
            f(&mut t);
            let o = match g.overrides.iter().position(|o| o.rig == rig) {
                Some(i) => &mut g.overrides[i],
                None => {
                    g.overrides.push(signal_guitar_proto::ToneOverride { rig: rig.clone(), parts: Vec::new(), tone: g.tone.clone() });
                    g.overrides.last_mut().expect("just pushed")
                }
            };
            if !o.parts.iter().any(|p| p == part) {
                o.parts.push(part.to_string());
            }
            match part {
                "trim" => o.tone.trim_db = t.trim_db,
                "gates" => o.tone.gates = t.gates,
                "noisy" => o.tone.noisy = t.noisy,
                _ => {
                    o.tone.low_cut_hz = t.low_cut_hz;
                    o.tone.bass_db = t.bass_db;
                    o.tone.mid_db = t.mid_db;
                    o.tone.treble_db = t.treble_db;
                }
            }
        }
    }
}

/// Write the rig's overrides of `parts` into the guitar (`keep`), or drop them.
fn settle(m: &mut SetupModel, parts: &[String], keep: bool) {
    let rig = audio_rig(m).id;
    let k = m.guitar_index as usize;
    let Some(g) = m.guitars.get_mut(k) else { return };
    let on = g.tone_on(&rig);
    if keep {
        for p in parts {
            match p.as_str() {
                "trim" => g.tone.trim_db = on.trim_db,
                "gates" => g.tone.gates = on.gates.clone(),
                "noisy" => g.tone.noisy = on.noisy,
                "eq" => {
                    g.tone.low_cut_hz = on.low_cut_hz;
                    g.tone.bass_db = on.bass_db;
                    g.tone.mid_db = on.mid_db;
                    g.tone.treble_db = on.treble_db;
                }
                _ => {}
            }
        }
    }
    if let Some(o) = g.overrides.iter_mut().find(|o| o.rig == rig) {
        o.parts.retain(|p| !parts.contains(p));
    }
    g.overrides.retain(|o| !o.parts.is_empty());
}

// ── The view ───────────────────────────────────────────────────────────────

#[component]
pub fn SetupView(state: RigViewState, tab: Signal<SetupTab>) -> Element {
    let client = use_hook(try_consume_context::<RigClient>);
    let model = use_signal(SetupModel::default);
    let rig = use_signal(|| client.clone());
    let photos = use_signal(std::collections::HashMap::new);
    let setup = use_context_provider(|| Setup { model, rig, photos });
    use_hook(move || {
        if let Some(r) = client.clone() {
            spawn(async move {
                if let Ok(m) = r.setup().await {
                    let mut model = model;
                    model.set(m);
                }
            });
        }
    });
    let _ = setup;
    let mut options = use_signal(|| false);
    rsx! {
        div { style: "flex: 1; min-width: 0; height: 100%; min-height: 0; display: flex; flex-direction: column; background: {DESK};",
            SetupTabs {
                tab: tab(),
                options: options(),
                on_tab: move |t: SetupTab| {
                    // A tap on the open tab brings up its options; another tab opens.
                    if t == tab() {
                        options.toggle();
                    } else {
                        let mut tab = tab;
                        tab.set(t);
                        options.set(false);
                    }
                },
            }
            div { style: "position: relative; flex: 1; min-height: 0; display: flex;",
                if options() {
                    SetupList { tab: tab(), on_picked: move |()| options.set(false) }
                }
                div { style: "flex: 1; min-width: 0; min-height: 0; overflow-y: auto;",
                    match tab() {
                        SetupTab::Guitar => rsx! { GuitarTab { state } },
                        SetupTab::Audio => rsx! { AudioTab { state } },
                        SetupTab::Midi => rsx! { MidiTab {} },
                    }
                }
            }
        }
    }
}

/// The three tabs, the full width: each says what it is and what is
/// chosen; the open one, tapped again, brings up its options.
#[component]
fn SetupTabs(tab: SetupTab, options: bool, on_tab: EventHandler<SetupTab>) -> Element {
    let setup = use_context::<Setup>();
    let m = setup.model.read().clone();
    let (g, r, c) = (guitar(&m), audio_rig(&m), controller(&m));
    let dev = MIDI_DEVICES.iter().find(|d| d.0 == c.device);
    let over: Vec<&str> = g.overridden_on(&r.id).iter().map(|p| tone_name(p)).collect();
    let tabs = [
        (SetupTab::Guitar, "Guitar", g.name.clone(), over.join(", ")),
        (SetupTab::Audio, "Audio", r.name.clone(), format!("{} · {} · {:.1} ms", khz(r.rate), r.buffer, latency_ms(&r))),
        (SetupTab::Midi, "MIDI", c.name.clone(), format!("{} · {}", c.device, dev.map_or("USB", |d| d.1))),
    ];
    rsx! {
        div { style: "flex-shrink: 0; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); height: {HEADER_H}px; border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box;",
            for (i, (id, label, name, sub)) in tabs.into_iter().enumerate() {
                {
                    let on = id == tab;
                    let border = if i > 0 { format!("border-left: 1px solid {RULE};") } else { String::new() };
                    let sub_ink = if id == SetupTab::Guitar { format!("color-mix(in oklab, {RIG} 70%, {INK})") } else { INK_3.to_string() };
                    rsx! {
                        button {
                            key: "{label}",
                            style: "position: relative; min-width: 0; display: flex; align-items: center; gap: 12px; padding: 0 16px; text-align: left; border: none; {border} background: {pick(on, ROW_ON, CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| on_tab.call(id),
                            if on {
                                span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: 3px; background: {INK};" }
                            }
                            span { style: "display: flex; opacity: {pick(on, 1.0, 0.55)};",
                                match id {
                                    SetupTab::Guitar => rsx! { GuitarPhoto { image: g.image.clone(), colour: g.colour.clone(), size: 48 } },
                                    SetupTab::Audio => rsx! { RigGlyph { on: true } },
                                    SetupTab::Midi => rsx! { ControllerGlyph { switches: dev.map_or(4, |d| d.2), on: true } },
                                }
                            }
                            span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                                span { style: "font-size: 11.5px; font-weight: 750; letter-spacing: 0.08em; text-transform: uppercase; color: {pick(on, INK_2, INK_3)};", "{label}" }
                                span { style: "font-size: 17px; font-weight: 750; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{name}" }
                                if !sub.is_empty() {
                                    span { style: "display: flex; align-items: center; gap: 6px; font-size: 12.5px; color: {sub_ink}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                        if id == SetupTab::Guitar { OverrideIcon { colour: RIG.to_string(), size: 11 } }
                                        "{sub}"
                                    }
                                }
                            }
                            if on {
                                svg { key: "{options}", width: "12", height: "8", view_box: "0 0 12 8", style: "flex-shrink: 0;",
                                    path { d: pick(options, "M1 6.5l5-5 5 5", "M1 1.5l5 5 5-5"), fill: "none", stroke: INK_2, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The open tab's options, down the left: the guitars, the rigs, the
/// controllers — the one in use marked green.
#[component]
fn SetupList(tab: SetupTab, on_picked: EventHandler<()>) -> Element {
    let setup = use_context::<Setup>();
    let host = PopupHost::try_use();
    let m = setup.model.read().clone();
    let r = audio_rig(&m);
    let taken_g: Vec<String> = m.guitars.iter().map(|g| g.name.clone()).collect();
    let taken_r: Vec<String> = m.rigs.iter().map(|g| g.name.clone()).collect();
    rsx! {
        aside { style: "width: 320px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; border-right: 1px solid {RULE}; background: {SHEET};",
            div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                match tab {
                    SetupTab::Guitar => rsx! {
                        for (i, g) in m.guitars.iter().cloned().enumerate() {
                            {
                                let over: Vec<&str> = g.overridden_on(&r.id).iter().map(|p| tone_name(p)).collect();
                                let sub = if over.is_empty() { String::new() } else { format!("{} on {}", over.join(", "), r.name) };
                                let only = m.guitars.len() <= 1;
                                rsx! {
                                    ListRow {
                                        key: "{g.id}",
                                        in_use: i == m.guitar_index as usize,
                                        title: g.name.clone(),
                                        sub,
                                        sub_mark: !over.is_empty(),
                                        on_pick: move |()| {
                                            setup.edit(|m| m.guitar_index = i as u32);
                                            on_picked.call(());
                                        },
                                        items: vec![
                                            Item::head(g.name.clone()),
                                            Item::name("rename", "Rename…", g.name.clone(), "Rename", taken_g.clone()),
                                            Item::Sep,
                                            Item::delete("delete", "Delete guitar").unless(only.then(|| "The only guitar".to_string())),
                                        ],
                                        on_menu: move |p: Picked| match p.id.as_str() {
                                            "rename" => setup.edit(|m| if let Some(g) = m.guitars.get_mut(i) {
                                                // Renamed is chosen: the one you named is the one in use.
                                                g.name = p.text.clone();
                                                m.guitar_index = i as u32;
                                            }),
                                            "delete" => setup.edit(|m| {
                                                if m.guitars.len() > 1 {
                                                    m.guitars.remove(i);
                                                    let k = m.guitar_index as usize;
                                                    m.guitar_index = (if k > i { k - 1 } else { k }).min(m.guitars.len() - 1) as u32;
                                                }
                                            }),
                                            _ => {}
                                        },
                                        GuitarPhoto { image: g.image.clone(), colour: g.colour.clone(), size: 52 }
                                    }
                                }
                            }
                        }
                    },
                    SetupTab::Audio => rsx! {
                        for (i, x) in m.rigs.iter().cloned().enumerate() {
                            {
                                let only = m.rigs.len() <= 1;
                                let in_use = i == m.rig_index as usize;
                                rsx! {
                                    ListRow {
                                        key: "{x.id}",
                                        in_use,
                                        title: x.name.clone(),
                                        sub: format!("{} · {} · {:.1} ms", khz(x.rate), x.buffer, latency_ms(&x)),
                                        sub_mark: false,
                                        on_pick: move |()| {
                                            setup.edit(|m| m.rig_index = i as u32);
                                            on_picked.call(());
                                        },
                                        items: vec![
                                            Item::head(x.name.clone()),
                                            Item::name("rename", "Rename…", x.name.clone(), "Rename", taken_r.clone()),
                                            Item::Sep,
                                            Item::delete("delete", "Delete rig").unless(only.then(|| "The only rig".to_string())),
                                        ],
                                        on_menu: move |p: Picked| match p.id.as_str() {
                                            "rename" => setup.edit(|m| if let Some(r) = m.rigs.get_mut(i) {
                                                r.name = p.text.clone();
                                                m.rig_index = i as u32;
                                            }),
                                            "delete" => setup.edit(|m| {
                                                if m.rigs.len() > 1 {
                                                    let gone = m.rigs.remove(i).id;
                                                    // Its overrides go with it.
                                                    for g in &mut m.guitars {
                                                        g.overrides.retain(|o| o.rig != gone);
                                                    }
                                                    let k = m.rig_index as usize;
                                                    m.rig_index = (if k > i { k - 1 } else { k }).min(m.rigs.len() - 1) as u32;
                                                }
                                            }),
                                            _ => {}
                                        },
                                        RigGlyph { on: in_use }
                                    }
                                }
                            }
                        }
                    },
                    SetupTab::Midi => rsx! {
                        for (i, c) in m.controllers.iter().cloned().enumerate() {
                            {
                                let dev = MIDI_DEVICES.iter().find(|d| d.0 == c.device);
                                let in_use = i == m.controller_index as usize;
                                rsx! {
                                    ListRow {
                                        key: "{c.id}",
                                        in_use,
                                        title: c.name.clone(),
                                        sub: format!("{} · {}", c.device, dev.map_or("USB", |d| d.1)),
                                        sub_mark: false,
                                        on_pick: move |()| {
                                            setup.edit(|m| m.controller_index = i as u32);
                                            on_picked.call(());
                                        },
                                        items: Vec::new(),
                                        on_menu: move |_| {},
                                        ControllerGlyph { switches: dev.map_or(4, |d| d.2), on: in_use }
                                    }
                                }
                            }
                        }
                    },
                }
                if tab != SetupTab::Midi {
                    button {
                        style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 52px; padding: 0 16px; border: none; border-top: 1px solid {RULE}; background: transparent; color: {INK_3}; font-size: 14px; font-weight: 600; font-family: {FONT}; text-align: left; cursor: pointer;",
                        onclick: move |e: MouseEvent| {
                            let (c, el) = (e.client_coordinates(), e.element_coordinates());
                            let item = if tab == SetupTab::Guitar {
                                Item::name("add", "New guitar…", "New guitar", "Add", taken_g.clone())
                            } else {
                                Item::name("add", "New audio rig — a copy of this one…", "New rig", "Add", taken_r.clone())
                            };
                            open_menu(host, c.x - el.x + 12.0, c.y - el.y + 48.0, vec![item], EventHandler::new(move |p: Picked| {
                                let id = format!("x{}", setup.model.peek().guitars.len() + setup.model.peek().rigs.len() + 1);
                                if tab == SetupTab::Guitar {
                                    setup.edit(|m| {
                                        m.guitars.push(GuitarEntry {
                                            id: format!("g-{id}-{}", p.text.to_lowercase()),
                                            name: p.text.clone(),
                                            colour: "#71717a".into(),
                                            pickups: vec![Pickup { position: "Bridge".into(), model: String::new() }],
                                            tone: GuitarTone { gates: gates_above(-74.0), low_cut_hz: 70.0, ..GuitarTone::default() },
                                            ..GuitarEntry::default()
                                        });
                                        m.guitar_index = (m.guitars.len() - 1) as u32;
                                    });
                                } else {
                                    setup.edit(|m| {
                                        let mut r = audio_rig(m);
                                        r.id = format!("r-{id}-{}", p.text.to_lowercase());
                                        r.name = p.text.clone();
                                        m.rigs.push(r);
                                        m.rig_index = (m.rigs.len() - 1) as u32;
                                    });
                                }
                            }));
                        },
                        Plus {}
                        if tab == SetupTab::Guitar { "New guitar" } else { "New audio rig" }
                    }
                }
                div { style: "border-top: 1px solid {RULE};" }
            }
        }
    }
}

/// A row in the list: the one in use marked green.
#[component]
fn ListRow(in_use: bool, title: String, sub: String, sub_mark: bool, on_pick: EventHandler<()>, items: Vec<Item>, on_menu: EventHandler<Picked>, children: Element) -> Element {
    let has_menu = !items.is_empty();
    rsx! {
        div { style: "position: relative; display: flex; align-items: center; border-top: 1px solid {RULE};",
            if in_use {
                span { style: "position: absolute; left: 0; top: 8px; bottom: 8px; width: 3px; border-radius: 0 2px 2px 0; background: {LIVE};" }
            }
            button {
                style: "flex: 1; min-width: 0; min-height: 68px; display: flex; align-items: center; gap: 12px; padding: 8px 4px 8px 14px; border: none; background: transparent; color: {INK}; text-align: left; font-family: {FONT}; cursor: pointer;",
                onclick: move |_| on_pick.call(()),
                {children}
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                    span { style: "font-size: 15px; font-weight: {pick(in_use, 700, 600)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{title}" }
                    if !sub.is_empty() {
                        span { style: "display: flex; align-items: center; gap: 6px; font-size: 12.5px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                            if sub_mark { OverrideIcon { colour: RIG.to_string(), size: 11 } }
                            "{sub}"
                        }
                    }
                }
            }
            if has_menu {
                MoreButton { label: format!("{title} actions"), items, on_pick: move |p| on_menu.call(p) }
            }
        }
    }
}

// ── Guitar ─────────────────────────────────────────────────────────────────

#[component]
fn GuitarTab(state: RigViewState) -> Element {
    let setup = use_context::<Setup>();
    let host = PopupHost::try_use();
    let m = setup.model.read().clone();
    let (g, r) = (guitar(&m), audio_rig(&m));
    let over = g.overridden_on(&r.id);
    let mut scope = use_signal(|| Scope::Guitar);
    let gid = g.id.clone();
    let mut last = use_signal(String::new);
    if *last.peek() != gid {
        last.set(gid);
        scope.set(Scope::Guitar);
    }
    let sc = scope();
    let tone = if sc == Scope::Guitar { g.tone.clone() } else { g.tone_on(&r.id) };
    let over_names: Vec<&str> = over.iter().map(|p| tone_name(p)).collect();
    let over_ink = format!("color-mix(in oklab, {RIG} 70%, {INK})");
    let gates_was = format!("Default {} dB{}", trim0(f64::from(g.tone.gates.get(1).copied().unwrap_or(-70.0))), if g.tone.noisy { " · noisy" } else { "" });
    rsx! {
        div { style: "display: flex; align-items: stretch; min-height: 100%;",
            // The guitar: its photo, upright, and its pickups.
            aside { style: "width: 232px; flex-shrink: 0; padding: 16px; display: flex; flex-direction: column; gap: 14px; border-right: 1px solid {RULE}; box-sizing: border-box;",
                GuitarPhoto { image: g.image.clone(), colour: g.colour.clone(), size: 0 }
                span { style: "font-size: 20px; font-weight: 800; line-height: 1.15;", "{g.name}" }
                div { style: "display: flex; flex-direction: column;",
                    for (i, p) in g.pickups.iter().cloned().enumerate() {
                        div { key: "{i}", style: "display: flex; flex-direction: column; gap: 4px; padding: 8px 0; border-top: 1px solid {RULE};",
                            span { style: "font-size: 11.5px; font-weight: 750; letter-spacing: 0.07em; text-transform: uppercase; color: {INK_3};", "{p.position}" }
                            TextField { value: p.model.clone(), placeholder: "Pickup model", on_commit: move |v: String| setup.edit(|m| {
                                let k = m.guitar_index as usize;
                                if let Some(q) = m.guitars.get_mut(k).and_then(|g| g.pickups.get_mut(i)) { q.model = v.clone() }
                            }) }
                        }
                    }
                    button {
                        style: "display: flex; align-items: center; gap: 8px; min-height: 44px; width: 100%; border: none; border-top: 1px solid {RULE}; background: transparent; color: {INK_3}; font-size: 14px; font-weight: 600; font-family: {FONT}; cursor: pointer;",
                        onclick: move |e: MouseEvent| {
                            let (c, el) = (e.client_coordinates(), e.element_coordinates());
                            open_menu(host, c.x - el.x, c.y - el.y + 44.0, vec![Item::name("add", "Pickup position…", "Middle", "Add", Vec::new())], EventHandler::new(move |p: Picked| setup.edit(|m| {
                                let k = m.guitar_index as usize;
                                if let Some(g) = m.guitars.get_mut(k) { g.pickups.push(Pickup { position: p.text.clone(), model: String::new() }) }
                            })));
                        },
                        Plus {}
                        "Pickup"
                    }
                }
            }
            div { style: "flex: 1; min-width: 0;",
                // Where a change lands.
                div { style: "position: sticky; top: 0px; z-index: 2; display: flex; align-items: center; gap: 12px; min-height: 60px; padding: 8px 20px; background: {SHEET}; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                    span { style: "display: flex; gap: 4px;",
                        for s in [Scope::Guitar, Scope::Rig] {
                            {
                                let on = s == sc;
                                let bg = match (on, s) {
                                    (false, _) => CLEAR.to_string(),
                                    (true, Scope::Rig) => format!("color-mix(in oklab, {RIG} 20%, transparent)"),
                                    (true, Scope::Guitar) => "rgba(255,255,255,0.08)".to_string(),
                                };
                                let label = if s == Scope::Guitar { "Guitar default".to_string() } else { format!("Only on {}", r.name) };
                                rsx! {
                                    button {
                                        key: "{label}",
                                        style: "height: 44px; padding: 0 14px; display: flex; align-items: center; gap: 8px; border: none; border-radius: {R}; font-size: 14px; font-weight: {pick(on, 750, 600)}; white-space: nowrap; color: {pick(on, INK, INK_3)}; background: {bg}; font-family: {FONT}; cursor: pointer;",
                                        onclick: move |_| scope.set(s),
                                        if s == Scope::Rig { OverrideIcon { key: "{on}", colour: pick(on, RIG, INK_3).to_string(), size: 12 } }
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                    if !over.is_empty() {
                        span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 13px; font-weight: 650; color: {over_ink};",
                            OverrideIcon { colour: RIG.to_string(), size: 12 }
                            "{over_names.join(\", \")}"
                        }
                    }
                }
                ToneBlock { title: "Trim", parts: vec!["trim".to_string()], over: over.clone(), scope: sc, rig: r.name.clone(), was: format!("{} dB", signed(f64::from(g.tone.trim_db))),
                    LevelMatch { state, tone: tone.clone(), scope: sc, target: f64::from(r.target_db) }
                }
                ToneBlock { title: "Gate", parts: vec!["gates".to_string(), "noisy".to_string()], over: over.clone(), scope: sc, rig: r.name.clone(), was: gates_was,
                    Gates { state, tone: tone.clone(), scope: sc }
                }
                ToneBlock { title: "Input EQ", parts: vec!["eq".to_string()], over: over.clone(), scope: sc, rig: r.name.clone(), was: format!("Low cut {} Hz", trim0(f64::from(g.tone.low_cut_hz))),
                    GuitarEq { tone: tone.clone(), scope: sc }
                }
            }
        }
    }
}

/// A part of the guitar's tone: its title, and whether the rig overrides
/// it — with Save to guitar and Discard, as a section's override.
#[component]
fn ToneBlock(title: &'static str, parts: Vec<String>, over: Vec<String>, scope: Scope, rig: String, was: String, children: Element) -> Element {
    let setup = use_context::<Setup>();
    let mine: Vec<String> = over.into_iter().filter(|p| parts.contains(p)).collect();
    let showing = scope == Scope::Rig && !mine.is_empty();
    let bg = if showing { format!("color-mix(in oklab, {RIG} 5%, transparent)") } else { CLEAR.to_string() };
    let note = if scope == Scope::Rig { format!("Override on {rig} · guitar: {was}") } else { format!("Overridden on {rig} — showing the guitar's") };
    let (m1, m2) = (mine.clone(), mine.clone());
    rsx! {
        section { style: "position: relative; padding: 18px 20px 22px; border-bottom: 1px solid {RULE}; background: {bg};",
            if showing {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {RIG};" }
            }
            div { style: "display: flex; align-items: center; gap: 12px; margin-bottom: 14px; min-height: 36px;",
                span { style: "font-size: 19px; font-weight: 750;", "{title}" }
                if !mine.is_empty() {
                    span { style: "display: inline-flex; align-items: center; gap: 7px; font-size: 13px; font-weight: 650; color: color-mix(in oklab, {RIG} 70%, {INK}); white-space: nowrap; overflow: hidden;",
                        OverrideIcon { colour: RIG.to_string(), size: 13 }
                        "{note}"
                    }
                }
                span { style: "flex: 1;" }
                if !mine.is_empty() {
                    span { style: "display: flex; gap: 6px;",
                        button {
                            style: "height: 36px; padding: 0 12px; border: none; border-radius: {R}; background: transparent; font-size: 13px; font-weight: 650; color: {INK_2}; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| setup.edit(|m| settle(m, &m1, false)),
                            "Discard"
                        }
                        button {
                            style: "height: 36px; padding: 0 14px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; font-size: 13px; font-weight: 700; color: {INK}; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| setup.edit(|m| settle(m, &m2, true)),
                            "Save to guitar"
                        }
                    }
                }
            }
            {children}
        }
    }
}

/// The guitar's level in dBFS after the trim, from the rig's input meter.
fn level_db(state: RigViewState, trim_db: f32) -> f64 {
    let lin = state.in_level.read().clamp(0.0, 1.0);
    // The meter is perceptual (sqrt-curved): back to amplitude, then dB.
    let amp = lin * lin;
    let db = if amp <= 1e-5 { -90.0 } else { 20.0 * amp.log10() };
    (db + f64::from(trim_db)).min(0.0)
}

/// The trim, against the level: the guitar's level on a dBFS scale with the
/// band presets expect and its peak; a wide fader for the trim, and Match
/// level, which listens and trims the peaks into the band.
#[component]
fn LevelMatch(state: RigViewState, tone: GuitarTone, scope: Scope, target: f64) -> Element {
    let setup = use_context::<Setup>();
    let now = level_db(state, tone.trim_db);
    let mut held = use_signal(|| -90.0_f64);
    let peak = now.max(*held.peek() - 0.35);
    if (peak - *held.peek()).abs() > 0.01 {
        held.set(peak);
    }
    let mut matching = use_signal(|| None::<f64>);
    if let Some(max) = matching() {
        let raw = now - f64::from(tone.trim_db);
        if raw > max {
            matching.set(Some(raw));
        }
    }
    let pos = |db: f64| ((db + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0);
    let in_band = (peak - target).abs() <= 3.0;
    let trim = f64::from(tone.trim_db);
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 12px;",
            div { style: "display: flex; align-items: center; gap: 14px;",
                div { style: "flex: 1; min-width: 0;",
                    div { style: "position: relative; height: 22px; border-radius: 5px; background: {WELL}; overflow: hidden;",
                        span { style: "position: absolute; top: 0; bottom: 0; left: {pos(target - 3.0)}%; width: 10%; background: rgba(34,197,94,0.14); border: 1px solid rgba(34,197,94,0.45); box-sizing: border-box;" }
                        span { style: "position: absolute; top: 5px; bottom: 5px; left: 0; width: {pos(now.max(-60.0))}%; border-radius: 3px; background: linear-gradient(90deg, #15803d, #22c55e 70%, #eab308 90%, #f87171);" }
                        span { style: "position: absolute; top: 2px; bottom: 2px; left: calc({pos(peak.max(-60.0))}% - 1px); width: 2px; background: {pick(in_band, LIVE, THUMB)};" }
                    }
                    div { style: "position: relative; height: 14px; margin-top: 3px; font-size: 11px; color: {INK_3};",
                        for d in [-60, -45, -30, -15, 0] {
                            span { key: "{d}", style: "position: absolute; left: {pos(f64::from(d))}%; margin-left: {pick(d == -60, 0, pick(d == 0, -8, -10))}px;", "{tick(d)}" }
                        }
                    }
                }
                span { style: "width: 92px; flex-shrink: 0; display: flex; flex-direction: column; align-items: flex-end; gap: 1px;",
                    span { style: "font-size: 17px; font-weight: 800; color: {pick(in_band, LIVE, INK)};", "{peak.round()}" }
                    span { style: "font-size: 11px; color: {INK_3};", "peak · {trim0(target)}" }
                }
            }
            div { style: "display: flex; align-items: center; gap: 10px;",
                Fader { value: trim, min: -24.0, max: 24.0, step: 0.5, level: None, hot: false, unit: "dB", signed_value: true,
                    on_change: move |v: f64| setup.tune(|m| toning(m, scope, "trim", |t| t.trim_db = v as f32)),
                    on_end: move |()| setup.save(),
                }
                button {
                    disabled: matching().is_some(),
                    style: "flex-shrink: 0; height: 52px; min-width: 128px; padding: 0 16px; border-radius: 6px; border: {outline(!matching().is_some())}; font-size: 14px; font-weight: 700; white-space: nowrap; color: {pick(matching().is_some(), ON_LIVE, INK)}; background: {pick(matching().is_some(), LIVE, CLEAR)}; font-family: {FONT}; cursor: pointer;",
                    onclick: move |_| {
                        matching.set(Some(-90.0));
                        spawn(async move {
                            architect::platform::sleep(std::time::Duration::from_millis(4000)).await;
                            // Trim the measured peaks onto the target.
                            let max = matching.peek().unwrap_or(-90.0);
                            matching.set(None);
                            let v = ((target - max) * 2.0).round() / 2.0;
                            setup.edit(|m| toning(m, scope, "trim", |t| t.trim_db = v.clamp(-24.0, 24.0) as f32));
                        });
                    },
                    if matching().is_some() { "Play…" } else { "Match level" }
                }
            }
        }
    }
}

/// The four gate thresholds on one strip, over the live level: drag the
/// nearest marker, or step a value; Measure sets all four above the noise
/// floor. Noisy input lifts Off to Subtle and Subtle to Default.
#[component]
fn Gates(state: RigViewState, tone: GuitarTone, scope: Scope) -> Element {
    let setup = use_context::<Setup>();
    let bus = DragBus::try_use();
    let now = level_db(state, tone.trim_db);
    let mut width = use_signal(|| 600.0_f64);
    let mut measuring = use_signal(|| None::<f64>);
    if let Some(floor) = measuring()
        && now < floor
    {
        measuring.set(Some(now));
    }
    let gates: Vec<f64> = (0..4).map(|i| f64::from(tone.gates.get(i).copied().unwrap_or(-70.0))).collect();
    let pct = |db: f64| ((db - GATE_MIN) / (GATE_MAX - GATE_MIN) * 100.0).clamp(0.0, 100.0);
    // Keep the four in order, a dB apart.
    let place = move |m: &mut SetupModel, which: usize, db: f64| {
        toning(m, scope, "gates", |t| {
            while t.gates.len() < 4 {
                t.gates.push(-70.0);
            }
            let lo = if which > 0 { f64::from(t.gates[which - 1]) + 1.0 } else { GATE_MIN };
            let hi = if which < 3 { f64::from(t.gates[which + 1]) - 1.0 } else { GATE_MAX };
            t.gates[which] = db.clamp(lo, hi).round() as f32;
        });
    };
    let default_gate = gates[1];
    let g2 = gates.clone();
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 12px;",
            // The strip: the level moving, the four thresholds over it.
            div {
                style: "position: relative; height: 64px; border-radius: 6px; background: {WELL}; overflow: hidden; cursor: ew-resize;",
                onmounted: move |e| {
                    let el = e.data();
                    spawn(async move {
                        if let Ok(r) = el.get_client_rect().await {
                            width.set(r.width().max(100.0));
                        }
                    });
                },
                onpointerdown: move |e| {
                    e.prevent_default();
                    let (cx, ex) = (e.client_coordinates().x, e.element_coordinates().x);
                    let w = width();
                    let at = move |x: f64| GATE_MIN + (x / w) * (GATE_MAX - GATE_MIN);
                    let db = at(ex);
                    let which = (0..4).min_by(|&a, &b| (g2[a] - db).abs().total_cmp(&(g2[b] - db).abs())).unwrap_or(1);
                    let left = cx - ex;
                    let Some(bus) = bus else { return };
                    bus.begin(move |ev| match ev {
                        DragEvent::Move { x, .. } => setup.tune(|m| place(m, which, at(x - left))),
                        DragEvent::End => setup.save(),
                    });
                },
                span { style: "position: absolute; left: 0; top: 26px; bottom: 10px; width: {pct(now)}%; border-radius: 0 3px 3px 0; background: {pick(now >= default_gate, LEVEL_OPEN, DIM)};" }
                for (i, g) in gates.iter().copied().enumerate() {
                    {
                        let open = now >= g;
                        rsx! {
                            span { key: "{i}", style: "position: absolute; top: 0; bottom: 0; left: calc({pct(g)}% - 10px); width: 20px; display: flex; flex-direction: column; align-items: center; pointer-events: none;",
                                span { style: "margin-top: 5px; width: 20px; height: 17px; border-radius: 4px; display: flex; align-items: center; justify-content: center; font-size: 11px; font-weight: 800; color: {pick(open, ON_LIVE, INK)}; background: {pick(open, LIVE, KEY_DOWN)};", "{&GATE_NAMES[i][..1]}" }
                                span { style: "flex: 1; width: 2px; margin-top: 2px; background: {THUMB}; opacity: 0.85;" }
                            }
                        }
                    }
                }
            }
            // Each threshold, to step.
            div { style: "display: grid; grid-template-columns: repeat(4, minmax(0, 1fr));",
                for (i, g) in gates.iter().copied().enumerate() {
                    div { key: "{i}", style: "display: flex; align-items: center; gap: 2px; padding: 2px 2px 2px 12px; {left_rule(i > 0)}",
                        span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
                            span { style: "font-size: 11.5px; font-weight: 750; letter-spacing: 0.06em; text-transform: uppercase; color: {INK_3};", "{GATE_NAMES[i]}" }
                            span { style: "font-size: 16px; font-weight: 800;", "{trim0(g)}" }
                        }
                        for d in [-1.0_f64, 1.0] {
                            button {
                                key: "{d}",
                                style: "width: 44px; height: 44px; flex-shrink: 0; border: none; border-radius: 6px; background: transparent; display: flex; align-items: center; justify-content: center; color: {INK_2}; font-size: 20px; font-weight: 600; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| setup.edit(|m| place(m, i, g + d)),
                                if d < 0.0 { "−" } else { "+" }
                            }
                        }
                    }
                }
            }
            div { style: "display: flex; align-items: center; gap: 12px; min-height: 52px; border-top: 1px solid {RULE}; padding-top: 8px;",
                button {
                    disabled: measuring().is_some(),
                    style: "height: 44px; padding: 0 16px; border-radius: {R}; border: {outline(!measuring().is_some())}; font-size: 14px; font-weight: 700; white-space: nowrap; color: {pick(measuring().is_some(), ON_LIVE, INK)}; background: {pick(measuring().is_some(), LIVE, CLEAR)}; font-family: {FONT}; cursor: pointer;",
                    onclick: move |_| {
                        measuring.set(Some(0.0));
                        spawn(async move {
                            architect::platform::sleep(std::time::Duration::from_millis(3000)).await;
                            // The floor: the quietest the input got while nothing was played.
                            let floor = measuring.peek().unwrap_or(-80.0).round();
                            measuring.set(None);
                            setup.edit(|m| toning(m, scope, "gates", |t| t.gates = gates_above(floor)));
                        });
                    },
                    if measuring().is_some() { "Hands off the strings…" } else { "Measure noise floor" }
                }
                span { style: "flex: 1;" }
                span { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 2px;",
                    span { style: "font-size: 15px; font-weight: 700;", "Noisy input" }
                    if tone.noisy {
                        span { style: "font-size: 12px; color: {INK_3};", "Off → Subtle · Subtle → Default" }
                    }
                }
                Toggle { on: tone.noisy, on_flip: move |()| setup.edit(|m| toning(m, scope, "noisy", |t| t.noisy = !t.noisy)) }
            }
        }
    }
}

/// The input EQ: its curve, and a low cut and three bands to set.
#[component]
fn GuitarEq(tone: GuitarTone, scope: Scope) -> Element {
    let setup = use_context::<Setup>();
    let (w, h) = (400.0_f64, 90.0_f64);
    let y = |db: f32| h / 2.0 - (f64::from(db) / 12.0) * (h / 2.0 - 8.0);
    let cut = (f64::from(tone.low_cut_hz) - 20.0) / 180.0 * 70.0;
    let path = format!(
        "M0 {} C {} {}, {} {}, {} {} S {} {}, {} {} S {} {}, {} {}",
        h - 4.0,
        cut * 0.6,
        h - 4.0,
        cut,
        y(tone.bass_db),
        cut + 30.0,
        y(tone.bass_db),
        w * 0.45,
        y(tone.mid_db),
        w * 0.55,
        y(tone.mid_db),
        w * 0.85,
        y(tone.treble_db),
        w,
        y(tone.treble_db)
    );
    let bands: [(&str, &str, f64, f64, f64, f64); 4] = [
        ("lowcut", "Low cut", 20.0, 200.0, 5.0, f64::from(tone.low_cut_hz)),
        ("bass", "Bass", -12.0, 12.0, 0.5, f64::from(tone.bass_db)),
        ("mid", "Mid", -12.0, 12.0, 0.5, f64::from(tone.mid_db)),
        ("treble", "Treble", -12.0, 12.0, 0.5, f64::from(tone.treble_db)),
    ];
    rsx! {
        div { style: "display: flex; gap: 16px; align-items: stretch;",
            svg { view_box: "0 0 {w} {h}", preserve_aspect_ratio: "none", style: "flex: 1 1 260px; min-width: 0; height: 110px; border-radius: 6px; background: {WELL};",
                line { x1: "0", x2: "{w}", y1: "{h / 2.0}", y2: "{h / 2.0}", stroke: RULE_STRONG, stroke_dasharray: "3 4" }
                path { d: "{path}", fill: "none", stroke: LIVE, stroke_width: "2.2" }
            }
            div { style: "flex: 1 1 260px; min-width: 0; display: grid; grid-template-columns: repeat(4, minmax(0, 1fr));",
                for (i, (k, label, min, max, step, v)) in bands.into_iter().enumerate() {
                    div { key: "{k}", style: "display: flex; flex-direction: column; align-items: center; gap: 6px; padding: 4px 0; {left_rule(i > 0)}",
                        span { style: "font-size: 12px; font-weight: 700; color: {INK_3};", "{label}" }
                        VFader { value: v, min, max, step,
                            on_change: move |nv: f64| setup.tune(|m| toning(m, scope, "eq", |t| match k {
                                "lowcut" => t.low_cut_hz = nv as f32,
                                "bass" => t.bass_db = nv as f32,
                                "mid" => t.mid_db = nv as f32,
                                _ => t.treble_db = nv as f32,
                            })),
                            on_end: move |()| setup.save(),
                        }
                        span { style: "font-size: 13px; font-weight: 700;",
                            if k == "lowcut" { "{trim0(v)} Hz" } else { "{signed(v)} dB" }
                        }
                    }
                }
            }
        }
    }
}

// ── Audio ──────────────────────────────────────────────────────────────────

#[component]
fn AudioTab(state: RigViewState) -> Element {
    let setup = use_context::<Setup>();
    // The devices the system has now — a choice beside the known interfaces.
    let settings = use_hook(try_consume_context::<signal_guitar_proto::audio::AudioSettingsClient>);
    let mut present = use_signal(Vec::<String>::new);
    use_hook(move || {
        if let Some(s) = settings.clone() {
            spawn(async move {
                if let Ok(d) = s.devices().await {
                    let mut names: Vec<String> = d.inputs.iter().chain(d.outputs.iter()).map(|x| x.name.clone()).collect();
                    names.dedup();
                    let mut seen = Vec::new();
                    names.retain(|n| if seen.contains(n) { false } else { seen.push(n.clone()); true });
                    present.set(names);
                }
            });
        }
    });
    let m = setup.model.read().clone();
    let r = audio_rig(&m);
    let dev = interface_of(&r);
    let lat = latency_ms(&r);
    let lat_ink = if lat < 8.0 { LIVE } else if lat < 14.0 { WARN } else { VOID };
    let set = move |f: Box<dyn FnOnce(&mut AudioRigEntry)>| {
        setup.edit(move |m| {
            let k = m.rig_index as usize;
            if let Some(r) = m.rigs.get_mut(k) {
                f(r);
            }
        });
    };
    let rate = r.rate;
    let outs = dev.outputs.len() * 2 + dev.phones.len() * 2;
    // The device is there: the system lists it (the built-in one whenever
    // the rig plays through the system's own).
    let connected = present().iter().any(|n| *n == r.device) || (r.device == "Built-in" && *state.running.read());
    rsx! {
        // The interface: the device, and the three numbers that decide feel.
        section { style: "border-bottom: 1px solid {RULE};",
            div { style: "display: flex; align-items: center; gap: 14px; padding: 18px 20px 6px;",
                span { style: "font-size: 19px; font-weight: 750;", "Interface" }
                span { style: "display: inline-flex; align-items: center; gap: 7px; font-size: 13px; color: {INK_3};",
                    span { style: "width: 8px; height: 8px; border-radius: 999px; background: {pick(connected, LIVE, DIM)};" }
                    "{running_label(connected)} · {dev.inputs.len()} in · {outs} out"
                }
            }
            div { style: "display: grid; grid-template-columns: minmax(0, 1.6fr) repeat(3, minmax(0, 1fr));",
                Cell { label: "Device",
                    Select { label: "Interface", value: r.device.clone(), options: device_options(&present(), &r.device),
                        on_pick: move |v: String| set(Box::new(move |y| {
                            // A new device: keep what it can do, else its nearest.
                            y.device = v.clone();
                            let d = interface_of(y);
                            if !d.inputs.iter().any(|i| i.0 == y.input) { y.input = d.inputs[0].0.into() }
                            if !d.rates.contains(&y.rate) { y.rate = if d.rates.contains(&48_000) { 48_000 } else { d.rates[0] } }
                            if !d.buffers.contains(&y.buffer) { y.buffer = if d.buffers.contains(&128) { 128 } else { d.buffers[0] } }
                            if !d.outputs.contains(&y.house.as_str()) { y.house = d.outputs[0].into() }
                            if !d.phones.contains(&y.phones.as_str()) { y.phones = d.phones[0].into() }
                        })),
                    }
                }
                Cell { label: "Sample rate",
                    Select { label: "Sample rate", value: r.rate.to_string(), options: dev.rates.iter().map(|x| (x.to_string(), khz(*x), String::new())).collect(),
                        on_pick: move |v: String| set(Box::new(move |y| y.rate = v.parse().unwrap_or(48_000))) }
                }
                Cell { label: "Buffer",
                    Select { label: "Buffer size", value: r.buffer.to_string(), options: dev.buffers.iter().map(|b| (b.to_string(), format!("{b} samples"), format!("{:.1} ms", f64::from(*b) * 1000.0 / f64::from(rate.max(1))))).collect(),
                        on_pick: move |v: String| set(Box::new(move |y| y.buffer = v.parse().unwrap_or(128))) }
                }
                Cell { label: "Round trip",
                    span { style: "height: 44px; display: flex; align-items: baseline; gap: 5px;",
                        span { style: "font-size: 30px; font-weight: 800; color: {lat_ink}; line-height: 44px;", "{lat:.1}" }
                        span { style: "font-size: 14px; font-weight: 650; color: {INK_3};", "ms" }
                    }
                }
            }
        }
        // The guitar's input: the interface's inputs, each with its level.
        section { style: "padding: 18px 20px 20px; border-bottom: 1px solid {RULE};",
            div { style: "font-size: 19px; font-weight: 750; margin-bottom: 14px;", "Guitar input" }
            div { style: "margin: 0 -20px; display: grid; grid-template-columns: repeat({dev.inputs.len().min(4)}, minmax(0, 1fr)); border-top: 1px solid {RULE}; border-bottom: 1px solid {RULE};",
                for (i, (name, kind)) in dev.inputs.iter().copied().enumerate() {
                    {
                        let on = name == r.input;
                        // Only the guitar's input has signal; the rest sit at the floor.
                        let lvl = if on { *state.in_level.read() } else { 0.02 };
                        rsx! {
                            button {
                                key: "{name}",
                                style: "position: relative; display: flex; align-items: center; gap: 12px; min-height: 68px; padding: 10px 14px; text-align: left; border: none; {left_rule(i > 0)} background: {pick(on, LIVE_WASH, CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| if !on { set(Box::new(move |y| y.input = name.to_string())) },
                                if on {
                                    span { style: "position: absolute; left: 0; right: 0; top: 0; height: 3px; background: {LIVE};" }
                                }
                                span { style: "position: relative; width: 6px; height: 40px; border-radius: 2px; background: {WELL}; overflow: hidden; flex-shrink: 0;",
                                    span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: {(lvl * 100.0).min(100.0)}%; background: {pick(lvl > 0.9, VOID, LIVE)};" }
                                }
                                span { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                                    span { style: "font-size: 15px; font-weight: {pick(on, 750, 600)};", "{name}" }
                                    span { style: "font-size: 12px; color: {pick(on, LIVE, INK_3)}; font-weight: {pick(on, 650, 500)};", "{pick(on, GUITAR, kind)}" }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "display: flex; align-items: center; gap: 12px; margin-top: 6px; min-height: 52px;",
                span { style: "flex: 1; font-size: 15px; font-weight: 700;", "Direct monitor" }
                Toggle { on: r.direct_monitor, on_flip: move |()| set(Box::new(|y| y.direct_monitor = !y.direct_monitor)) }
            }
        }
        // Outputs: the house and the phones, each with its level and a check.
        section { style: "border-bottom: 1px solid {RULE};",
            div { style: "padding: 18px 20px 4px; font-size: 19px; font-weight: 750;", "Outputs" }
            div { style: "display: grid; grid-template-columns: repeat(2, minmax(0, 1fr));",
                OutputStrip { state, house: true }
                OutputStrip { state, house: false }
            }
        }
    }
}

/// One output: where it goes, its level over its meter, and a check that
/// lights left, right, then both.
#[component]
fn OutputStrip(state: RigViewState, house: bool) -> Element {
    let setup = use_context::<Setup>();
    let m = setup.model.read().clone();
    let r = audio_rig(&m);
    let dev = interface_of(&r);
    let db = f64::from(if house { r.house_db } else { r.phones_db });
    let level = state.out_level.read().clamp(0.0, 1.0) * 10f64.powf(db / 40.0);
    let fill = (db + 60.0) / 66.0;
    // The check: left, right, then both, 1.2 s each — a swapped cable or a
    // dead side shows. Otherwise the glyph lights while the rig plays.
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut check = use_signal(|| None::<u8>);
    let side = check().or((level > 0.02).then_some(2u8));
    let options: Vec<(String, String, String)> = if house { dev.outputs } else { dev.phones }.iter().map(|o| (o.to_string(), o.to_string(), String::new())).collect();
    let label = if house { "House" } else { "Phones" };
    let out_label: &'static str = if house { "House output" } else { "Phones output" };
    let out_value = if house { r.house.clone() } else { r.phones.clone() };
    rsx! {
        div { style: "padding: 12px 20px 20px; display: flex; flex-direction: column; gap: 12px; min-width: 0;",
            div { style: "display: flex; align-items: center; gap: 10px;",
                SideGlyph { house, side }
                span { style: "font-size: 16px; font-weight: 750;", "{label}" }
                span { style: "flex: 1;" }
                span { style: "width: 170px;",
                    Select { label: out_label, value: out_value, options,
                        on_pick: move |v: String| setup.edit(|m| {
                            let k = m.rig_index as usize;
                            if let Some(r) = m.rigs.get_mut(k) { if house { r.house = v.clone() } else { r.phones = v.clone() } }
                        }) }
                }
            }
            Fader { value: db, min: -60.0, max: 6.0, step: 0.5, level: Some((level.min(1.0) * fill).clamp(0.0, 1.0)), hot: level > 0.95, unit: "dB", signed_value: true,
                on_change: move |v: f64| setup.tune(|m| {
                    let k = m.rig_index as usize;
                    if let Some(r) = m.rigs.get_mut(k) { if house { r.house_db = v as f32 } else { r.phones_db = v as f32 } }
                }),
                on_end: move |()| setup.save(),
            }
            div { style: "display: flex; align-items: center; gap: 12px;",
                button {
                    disabled: check().is_some(),
                    style: "flex-shrink: 0; white-space: nowrap; height: 44px; padding: 0 16px; display: flex; align-items: center; gap: 8px; border-radius: {R}; font-size: 14px; font-weight: 700; font-family: {FONT}; cursor: pointer; box-sizing: border-box; color: {pick(check().is_some(), ON_LIVE, INK)}; background: {pick(check().is_some(), LIVE, CLEAR)}; border: 1px solid {pick(check().is_some(), LIVE, RULE_STRONG)};",
                    onclick: move |_| {
                        let output = if house { "house" } else { "phones" }.to_string();
                        let rig = rig.clone();
                        spawn(async move {
                            // Left (0), right (1), both (2).
                            for (shown, side) in [(0u8, 1u32), (1, 2), (2, 3)] {
                                check.set(Some(shown));
                                if let Some(r) = rig.clone() {
                                    let _ = r.test_tone(output.clone(), side).await;
                                }
                                architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
                            }
                            if let Some(r) = rig.clone() {
                                let _ = r.test_tone(output.clone(), 0).await;
                            }
                            check.set(None);
                        });
                    },
                    match check() {
                        Some(0) => "Left…",
                        Some(1) => "Right…",
                        Some(_) => "Both…",
                        None if house => "Check speakers",
                        None => "Check phones",
                    }
                }
            }
        }
    }
}

// ── MIDI ───────────────────────────────────────────────────────────────────

#[component]
fn MidiTab() -> Element {
    let setup = use_context::<Setup>();
    let m = setup.model.read().clone();
    let c = controller(&m);
    let dev = MIDI_DEVICES.iter().find(|d| d.0 == c.device).copied();
    // What the rig hears, newest first — polled while the tab is open.
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut heard = use_signal(Vec::<String>::new);
    // The switch just pressed, lit for a moment.
    let mut pressed = use_signal(|| None::<usize>);
    use_hook(move || {
        if let Some(r) = rig.clone() {
            spawn(async move {
                let mut seen: Option<(usize, String)> = None;
                let mut first = true;
                loop {
                    if let Ok(log) = r.midi_recent().await {
                        let newest = log.last().cloned().map(|l| (log.len(), l));
                        // Something new from a switch the map knows: light it.
                        if !first && newest != seen
                            && let Some((_, line)) = &newest
                            && let Some(n) = line.strip_prefix("Switch ").and_then(|x| x.split(' ').next()).and_then(|x| x.parse::<usize>().ok())
                        {
                            pressed.set(Some(n - 1));
                            spawn(async move {
                                architect::platform::sleep(std::time::Duration::from_millis(220)).await;
                                if pressed() == Some(n - 1) {
                                    pressed.set(None);
                                }
                            });
                        }
                        seen = newest.or(seen);
                        first = false;
                        let recent: Vec<String> = log.into_iter().rev().take(6).collect();
                        if *heard.peek() != recent {
                            heard.set(recent);
                        }
                    }
                    architect::platform::sleep(std::time::Duration::from_millis(400)).await;
                }
            });
        }
    });
    let set = move |f: Box<dyn FnOnce(&mut ControllerEntry)>| {
        setup.edit(move |m| {
            let k = m.controller_index as usize;
            if let Some(c) = m.controllers.get_mut(k) {
                f(c);
            }
        });
    };
    let mut channels = vec![("0".to_string(), "Omni · all".to_string(), String::new())];
    channels.extend((1..=16).map(|i| (i.to_string(), format!("Channel {i}"), String::new())));
    let clocks = vec![
        ("off".to_string(), "Off".to_string(), String::new()),
        ("send".to_string(), "Send tempo".to_string(), String::new()),
        ("receive".to_string(), "Follow it".to_string(), String::new()),
    ];
    rsx! {
        section { style: "border-bottom: 1px solid {RULE};",
            div { style: "display: flex; align-items: center; gap: 14px; padding: 18px 20px 6px;",
                span { style: "font-size: 19px; font-weight: 750;", "{c.device}" }
                if let Some(d) = dev {
                    span { style: "display: inline-flex; align-items: center; gap: 7px; font-size: 13px; color: {INK_3};",
                        span { style: "width: 8px; height: 8px; border-radius: 999px; background: {LIVE};" }
                        "{link_label(d.1)}"
                    }
                }
            }
            div { style: "display: grid; grid-template-columns: repeat(2, minmax(0, 1fr));",
                Cell { label: "Listens on",
                    Select { label: "MIDI channel", value: c.channel.to_string(), options: channels, on_pick: move |v: String| set(Box::new(move |y| y.channel = v.parse().unwrap_or(0))) }
                }
                Cell { label: "Clock",
                    Select { label: "MIDI clock", value: if c.clock.is_empty() { "off".to_string() } else { c.clock.clone() }, options: clocks, on_pick: move |v: String| set(Box::new(move |y| y.clock = v)) }
                }
            }
        }
        if let Some(d) = dev {
            section { style: "border-bottom: 1px solid {RULE};",
                div { style: "padding: 18px 20px 12px; font-size: 19px; font-weight: 750;", "Switches" }
                // Its switches in a row, flush: each lights when pressed.
                div { style: "display: grid; grid-template-columns: repeat({d.2}, minmax(0, 1fr)); border-top: 1px solid {RULE}; border-bottom: 1px solid {RULE};",
                    for i in 0..d.2 {
                        {
                            let on = pressed() == Some(i as usize);
                            rsx! {
                                div {
                                    key: "{i}",
                                    style: "position: relative; min-width: 0; display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 14px 0 12px; {left_rule(i > 0)}",
                                    if on {
                                        span { style: "position: absolute; left: 0; right: 0; top: 0; bottom: 0; background: rgba(34,197,94,0.1);" }
                                    }
                                    span { style: "width: 10px; height: 10px; border-radius: 999px; background: {pick(on, LIVE, RULE_STRONG)};" }
                                    span { style: "width: 40px; height: 40px; border-radius: 999px; background: {pick(on, KEY_DOWN, UP)}; border-bottom: 3px solid rgba(0,0,0,0.45); box-sizing: border-box;" }
                                    span { style: "font-size: 13px; font-weight: 700; color: {INK_3};", "{i + 1}" }
                                }
                            }
                        }
                    }
                }
                div { style: "padding: 4px 20px 12px; min-height: 44px; display: flex; flex-direction: column;",
                    for (i, h) in heard().into_iter().enumerate() {
                        span { key: "{i}-{h}", style: "display: flex; min-height: 36px; align-items: center; font-size: 14px; color: {pick(i == 0, INK, INK_3)}; {top_rule(i > 0)}", "{h}" }
                    }
                }
            }
        }
        section { style: "padding: 6px 20px 10px; border-bottom: 1px solid {RULE};",
            div { style: "display: flex; align-items: center; gap: 12px; min-height: 64px;",
                span { style: "flex: 1; font-size: 15px; font-weight: 700;", "Program changes" }
                Toggle { on: c.program_change, on_flip: move |()| set(Box::new(|y| y.program_change = !y.program_change)) }
            }
        }
    }
}

// ── Pieces ─────────────────────────────────────────────────────────────────

/// A labelled cell of the settings grid: a small label over its control.
#[component]
fn Cell(label: &'static str, children: Element) -> Element {
    rsx! {
        div { style: "min-width: 0; display: flex; flex-direction: column; gap: 8px; padding: 14px 18px 16px;",
            span { style: "font-size: 11.5px; font-weight: 750; letter-spacing: 0.07em; text-transform: uppercase; color: {INK_3};", "{label}" }
            {children}
        }
    }
}

/// A dropdown: the value and a chevron; it opens the choices as a menu.
/// `options`: (id, shown, detail).
#[component]
fn Select(label: &'static str, value: String, options: Vec<(String, String, String)>, on_pick: EventHandler<String>) -> Element {
    let host = PopupHost::try_use();
    let shown = options.iter().find(|o| o.0 == value).map_or(value.clone(), |o| o.1.clone());
    rsx! {
        button {
            style: "width: 100%; min-width: 0; height: 44px; padding: 0 12px 0 14px; display: flex; align-items: center; gap: 10px; border: none; border-radius: {R}; background: {FILL}; color: {INK}; text-align: left; font-family: {FONT}; cursor: pointer;",
            onclick: move |e: MouseEvent| {
                let (c, el) = (e.client_coordinates(), e.element_coordinates());
                let mut items = vec![Item::head(label)];
                items.extend(options.iter().map(|(id, show, detail)| {
                    let it = Item::run(id.clone(), show.clone()).checked(*id == value);
                    if detail.is_empty() { it } else { it.detail(detail.clone()) }
                }));
                open_menu(host, c.x - el.x, c.y - el.y + 48.0, items, EventHandler::new(move |p: Picked| on_pick.call(p.id)));
            },
            span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: 650; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{shown}" }
            svg { width: "11", height: "7", view_box: "0 0 11 7", style: "flex-shrink: 0;",
                path { d: "M1 1l4.5 4.5L10 1", fill: "none", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
            }
        }
    }
}

#[component]
fn Toggle(on: bool, on_flip: EventHandler<()>) -> Element {
    rsx! {
        button {
            style: "width: 52px; height: 44px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; border: none; background: transparent; cursor: pointer;",
            onclick: move |_| on_flip.call(()),
            span { style: "width: 46px; height: 28px; border-radius: 999px; padding: 3px; box-sizing: border-box; background: {pick(on, LIVE, RULE_STRONG)}; display: flex; justify-content: {pick(on, END, START)};",
                span { style: "width: 22px; height: 22px; border-radius: 999px; background: {THUMB};" }
            }
        }
    }
}

/// A wide touch fader: its value written in it, the signal along its foot.
#[component]
fn Fader(value: f64, min: f64, max: f64, step: f64, level: Option<f64>, hot: bool, unit: &'static str, signed_value: bool, on_change: EventHandler<f64>, on_end: EventHandler<()>) -> Element {
    let bus = DragBus::try_use();
    let mut width = use_signal(|| 300.0_f64);
    let fill = ((value - min) / (max - min)).clamp(0.0, 1.0) * 100.0;
    let shown = if unit == "dB" && value <= -60.0 && min <= -60.0 {
        "−∞".to_string()
    } else if signed_value {
        format!("{} {unit}", signed(value))
    } else {
        format!("{} {unit}", trim0(value))
    };
    rsx! {
        div {
            style: "position: relative; flex: 1; min-width: 0; height: 52px; border-radius: 6px; background: {WELL}; overflow: hidden; cursor: ew-resize;",
            onmounted: move |e| {
                let el = e.data();
                spawn(async move {
                    if let Ok(r) = el.get_client_rect().await {
                        width.set(r.width().max(40.0));
                    }
                });
            },
            onpointerdown: move |e| {
                e.prevent_default();
                let (cx, ex) = (e.client_coordinates().x, e.element_coordinates().x);
                let left = cx - ex;
                let w = width();
                let at = move |x: f64| (min + ((x - left) / w) * (max - min)).clamp(min, max);
                let snap = move |v: f64| (v / step).round() * step;
                on_change.call(snap(at(cx)));
                let Some(bus) = bus else {
                    on_end.call(());
                    return;
                };
                bus.begin(move |ev| match ev {
                    DragEvent::Move { x, .. } => on_change.call(snap(at(x))),
                    DragEvent::End => on_end.call(()),
                });
            },
            span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: {fill}%; background: linear-gradient(90deg, #1e1e24, #2a2a31);" }
            if let Some(l) = level {
                span { style: "position: absolute; left: 0; bottom: 0; height: 4px; width: {l * 100.0}%; background: {pick(hot, VOID, LIVE)};" }
            }
            span { style: "position: absolute; top: 8px; bottom: 8px; left: calc({fill}% - 2px); width: 4px; border-radius: 2px; background: {THUMB};" }
            span { style: "position: absolute; left: 14px; top: 0; bottom: 0; display: flex; align-items: center; font-size: 18px; font-weight: 800; pointer-events: none;", "{shown}" }
        }
    }
}

/// An upright fader: an EQ band.
#[component]
fn VFader(value: f64, min: f64, max: f64, step: f64, on_change: EventHandler<f64>, on_end: EventHandler<()>) -> Element {
    let bus = DragBus::try_use();
    const H: f64 = 70.0;
    let fill = ((value - min) / (max - min)).clamp(0.0, 1.0);
    rsx! {
        div {
            style: "position: relative; width: 44px; height: {H}px; display: flex; justify-content: center; cursor: ns-resize;",
            onpointerdown: move |e| {
                e.prevent_default();
                let (cy, ey) = (e.client_coordinates().y, e.element_coordinates().y);
                let top = cy - ey;
                let at = move |y: f64| (max - ((y - top) / H) * (max - min)).clamp(min, max);
                let snap = move |v: f64| (v / step).round() * step;
                on_change.call(snap(at(cy)));
                let Some(bus) = bus else {
                    on_end.call(());
                    return;
                };
                bus.begin(move |ev| match ev {
                    DragEvent::Move { y, .. } => on_change.call(snap(at(y))),
                    DragEvent::End => on_end.call(()),
                });
            },
            span { style: "position: absolute; top: 0; bottom: 0; left: 20px; width: 4px; border-radius: 2px; background: {WELL};" }
            span { style: "position: absolute; bottom: 0; left: 20px; width: 4px; height: {fill * 100.0}%; border-radius: 2px; background: {LIVE};" }
            span { style: "position: absolute; left: 12px; width: 20px; height: 8px; top: {(1.0 - fill) * (H - 8.0)}px; border-radius: 3px; background: {THUMB};" }
        }
    }
}

/// A text field that commits on change (Blitz has no blur event to wait for).
#[component]
fn TextField(value: String, placeholder: &'static str, on_commit: EventHandler<String>) -> Element {
    rsx! {
        input {
            value: "{value}",
            placeholder,
            style: "width: 100%; min-width: 0; height: 40px; flex-shrink: 0; padding: 0 12px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK}; font-size: 15px; font-family: {FONT}; box-sizing: border-box;",
            onchange: move |e| {
                let v = e.value().trim().to_string();
                if !v.is_empty() {
                    on_commit.call(v);
                }
            },
        }
    }
}

/// A guitar's photo, cropped to its body — fetched once from the rig; its
/// finish behind a guitar glyph when it has none. `size` 0: the sidebar's
/// tall picture.
#[component]
fn GuitarPhoto(image: String, colour: String, size: u32) -> Element {
    let setup = use_context::<Setup>();
    let wanted = image.clone();
    use_effect(use_reactive!(|wanted| {
        if wanted.is_empty() || setup.photos.peek().contains_key(&wanted) {
            return;
        }
        let mut photos = setup.photos;
        photos.write().insert(wanted.clone(), String::new());
        if let Some(r) = setup.rig.peek().clone() {
            spawn(async move {
                let bytes = r.guitar_photo(wanted.clone()).await.unwrap_or_default();
                if !bytes.is_empty() {
                    use base64::Engine as _;
                    let mime = match wanted.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
                        "png" => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        _ => "image/webp",
                    };
                    let url = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes));
                    photos.write().insert(wanted, url);
                }
            });
        }
    }));
    let url = setup.photos.read().get(&image).cloned().unwrap_or_default();
    let colour = if colour.is_empty() { "#71717a".to_string() } else { colour };
    let (w, h, r) = if size == 0 { ("100%".to_string(), "260px".to_string(), 10) } else { (format!("{size}px"), format!("{size}px"), 8) };
    if !url.is_empty() {
        return rsx! {
            img { src: "{url}", style: "width: {w}; height: {h}; flex-shrink: 0; border-radius: {r}px; display: block; object-fit: cover; object-position: 50% 62%; background: {colour};" }
        };
    }
    rsx! {
        span { style: "width: {w}; height: {h}; flex-shrink: 0; border-radius: {r}px; background: {colour}; border: 1px solid rgba(255,255,255,0.12); box-sizing: border-box; display: flex; align-items: center; justify-content: center;",
            svg { width: "45%", height: "45%", view_box: "0 0 24 24",
                path { d: "M19 2l3 3-6.5 6.5M15.5 11.5 12.5 8.5", fill: "none", stroke: "rgba(128,128,128,0.8)", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
                path { d: "M12.5 8.5c-2-1.2-4.6-.6-5.4 1.4-.3.8-1 1.3-1.9 1.4-2.3.3-3.6 3.2-1.9 5.3l2.9 2.9c2.1 1.7 5 .4 5.3-1.9.1-.9.6-1.6 1.4-1.9 2-.8 2.6-3.4 1.4-5.4", fill: "none", stroke: "rgba(128,128,128,0.8)", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
            }
        }
    }
}

/// An interface, front on: two inputs and a knob.
#[component]
fn RigGlyph(on: bool) -> Element {
    let c = pick(on, INK_2, INK_3);
    rsx! {
        span { style: "width: 52px; height: 52px; flex-shrink: 0; display: flex; align-items: center; justify-content: center;",
            svg { key: "{on}", width: "34", height: "20", view_box: "0 0 30 18",
                rect { x: "1", y: "1", width: "28", height: "16", rx: "3", fill: "none", stroke: c, stroke_width: "1.5" }
                circle { cx: "8", cy: "9", r: "3", fill: "none", stroke: c, stroke_width: "1.5" }
                circle { cx: "16", cy: "9", r: "3", fill: "none", stroke: c, stroke_width: "1.5" }
                circle { cx: "24", cy: "9", r: "2", fill: c }
            }
        }
    }
}

/// A floor controller, top down: its switches in a row.
#[component]
fn ControllerGlyph(switches: u32, on: bool) -> Element {
    let c = pick(on, INK_2, INK_3);
    let w = 6 + switches * 6;
    let px = (f64::from(w) * 1.2).min(40.0);
    rsx! {
        span { style: "width: 52px; height: 52px; flex-shrink: 0; display: flex; align-items: center; justify-content: center;",
            svg { key: "{on}", width: "{px}", height: "16", view_box: "0 0 {w} 12",
                rect { x: "0.6", y: "0.6", width: "{f64::from(w) - 1.2}", height: "10.8", rx: "2", fill: "none", stroke: c, stroke_width: "1.2" }
                for i in 0..switches {
                    circle { key: "{i}", cx: "{6 + i * 6}", cy: "6", r: "1.8", fill: c }
                }
            }
        }
    }
}

/// Speakers or phones, each side lit while the check plays it.
#[component]
fn SideGlyph(house: bool, side: Option<u8>) -> Element {
    let lit = |s: u8| if side == Some(s) || side == Some(2) { LIVE } else { DIM };
    let (l, r) = (lit(0), lit(1));
    rsx! {
        svg { key: "{side:?}", width: "30", height: "26", view_box: "0 0 30 26", style: "flex-shrink: 0;",
            if house {
                rect { x: "1", y: "2", width: "12", height: "22", rx: "2.5", fill: "none", stroke: l, stroke_width: "1.8" }
                circle { cx: "7", cy: "16", r: "3.6", fill: l }
                circle { cx: "7", cy: "7.5", r: "1.8", fill: l }
                rect { x: "17", y: "2", width: "12", height: "22", rx: "2.5", fill: "none", stroke: r, stroke_width: "1.8" }
                circle { cx: "23", cy: "16", r: "3.6", fill: r }
                circle { cx: "23", cy: "7.5", r: "1.8", fill: r }
            } else {
                path { d: "M4 16v-3a11 11 0 0 1 22 0v3", fill: "none", stroke: INK_3, stroke_width: "2", stroke_linecap: "round" }
                rect { x: "2", y: "15", width: "7", height: "10", rx: "2.5", fill: l }
                rect { x: "21", y: "15", width: "7", height: "10", rx: "2.5", fill: r }
            }
        }
    }
}

#[component]
fn Plus() -> Element {
    rsx! {
        svg { width: "12", height: "12", view_box: "0 0 12 12",
            path { d: "M6 1v10M1 6h10", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
        }
    }
}
