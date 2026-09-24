//! Audio settings — device / channel / buffer / sample-rate picker.
//!
//! Pure presentation over the proto types: the hosting view fetches devices
//! and prefs over the `AudioSettings` service, builds an [`AudioSettingsBridge`]
//! carrying them plus a save callback, and hands it to the modal.

use dioxus::prelude::*;
use signal_guitar_proto::{AudioDevice, AudioPrefs};

/// Devices + prefs + save callback handed to the settings modal / pickers.
///
/// Built per-render by the hosting view from the fetched device lists and the
/// shared prefs signal; edits round-trip through
/// [`on_save`](AudioSettingsBridge::on_save) (persist over RPC + rig restart).
#[derive(Clone, PartialEq)]
pub struct AudioSettingsBridge {
    pub inputs: Vec<AudioDevice>,
    pub outputs: Vec<AudioDevice>,
    pub prefs: AudioPrefs,
    pub on_save: Callback<AudioPrefs>,
}

/// Audio Settings is open — one switch for the whole window, so the macOS
/// menu (Audio Settings… ⌘,), the bar's Audio indicator and the phones
/// strip all open the same page.
pub static AUDIO_SETTINGS_OPEN: GlobalSignal<bool> = Signal::global(|| false);

/// Open Audio Settings.
pub fn open_audio_settings() {
    *AUDIO_SETTINGS_OPEN.write() = true;
}

/// Selectable buffer sizes (frames) — matches the guitar TUI's `BUFFERS`.
pub(crate) const BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];

/// Selectable sample rates (Hz). `0` = device native.
const SAMPLE_RATES: &[(u32, &str)] = &[
    (0, "Device native"),
    (44_100, "44.1 kHz"),
    (48_000, "48 kHz"),
    (88_200, "88.2 kHz"),
    (96_000, "96 kHz"),
];

/// Channels to offer when a device's count is unknown (the "system default"
/// entry, or a device that is not plugged in right now).
const DEFAULT_MAX_CHANNELS: u16 = 8;

/// Human label for a device value (empty string → "System default").
fn device_label(name: &str) -> String {
    if name.is_empty() {
        "System default".to_string()
    } else {
        name.to_string()
    }
}

/// The channel count of the device a pref names (a name substring, as the
/// rig matches it).
fn channels_of(devices: &[AudioDevice], name: &str) -> u16 {
    let want = name.to_lowercase();
    devices
        .iter()
        .find(|d| !want.is_empty() && d.name.to_lowercase().contains(&want))
        .map_or(DEFAULT_MAX_CHANNELS, |d| d.channels.max(1))
}

/// Stereo pairs `1-2`, `3-4`, … across `channels`, as `(label, (l, r))`.
fn pairs(channels: u16) -> Vec<(String, (u32, u32))> {
    (0..u32::from(channels.max(2)) / 2)
        .map(|k| (format!("{}-{}", 2 * k + 1, 2 * k + 2), (2 * k, 2 * k + 1)))
        .collect()
}

/// The mix input choices: every stereo pair, then every channel alone (a
/// mono mix, in both ears).
fn mix_inputs(channels: u16) -> Vec<(String, (u32, u32))> {
    let mut out: Vec<(String, (u32, u32))> = pairs(channels)
        .into_iter()
        .map(|(l, p)| (format!("{l} (stereo)"), p))
        .collect();
    out.extend((0..u32::from(channels.max(1))).map(|c| (format!("{} (mono)", c + 1), (c, c))));
    out
}

/// The Audio Settings page: the interface, where the main mix and the
/// phones go, and the headphone mix. Device and routing edits are staged and
/// applied together (the device reopens — a short gap); the phones levels
/// move live.
///
/// It replaces the rig's body while open (the host hides the body), rather
/// than floating over it: an overlay above the live rig costs every frame
/// both, and Blitz has no `position: fixed` to pin one anyway.
#[component]
pub fn AudioSettingsModal(
    bridge: AudioSettingsBridge,
    on_close: EventHandler<()>,
    /// The live rig — for the phones levels, the mixer's state and the mix
    /// meter. Without it the page edits the device prefs only.
    #[props(default)]
    state: Option<crate::state::RigViewState>,
) -> Element {
    let base = bridge.prefs.clone();
    let mut edit = use_signal(|| base.clone());
    // New prefs from the rig (another remote saved) reseed an untouched page.
    {
        let base = base.clone();
        let mut seen = use_signal(|| base.clone());
        if *seen.peek() != base {
            if *edit.peek() == *seen.peek() {
                edit.set(base.clone());
            }
            seen.set(base);
        }
    }
    let p = edit();
    let dirty = p != bridge.prefs;
    let inputs = bridge.inputs.clone();
    let outputs = bridge.outputs.clone();
    let on_save = bridge.on_save;

    let in_ch = channels_of(&inputs, &p.input_device);
    let out_name = if p.output_device.is_empty() { p.input_device.clone() } else { p.output_device.clone() };
    let out_ch = channels_of(&outputs, &out_name);

    // Pickers: options + the selected index.
    let in_devs: Vec<String> = std::iter::once("System default".to_string())
        .chain(inputs.iter().map(|d| format!("{} ({} in)", d.name, d.channels)))
        .collect();
    let in_sel = if p.input_device.is_empty() {
        0
    } else {
        inputs.iter().position(|d| d.name.to_lowercase().contains(&p.input_device.to_lowercase())).map_or(u32::MAX, |i| i as u32 + 1)
    };
    let out_devs: Vec<String> = std::iter::once("Same as input".to_string())
        .chain(outputs.iter().map(|d| format!("{} ({} out)", d.name, d.channels)))
        .collect();
    let out_sel = if p.output_device.is_empty() {
        0
    } else {
        outputs.iter().position(|d| d.name.to_lowercase().contains(&p.output_device.to_lowercase())).map_or(u32::MAX, |i| i as u32 + 1)
    };
    let guitar_in: Vec<String> = (1..=in_ch).map(|c| format!("Input {c}")).collect();
    let rates: Vec<String> = SAMPLE_RATES.iter().map(|(_, l)| (*l).to_string()).collect();
    let rate_sel = SAMPLE_RATES.iter().position(|(hz, _)| *hz == p.sample_rate).map_or(u32::MAX, |i| i as u32);
    let bufs: Vec<String> = BUFFER_SIZES.iter().map(|b| format!("{b} frames · {:.1} ms", f64::from(*b) / 48.0)).collect();
    let buf_sel = BUFFER_SIZES.iter().position(|b| *b == p.buffer_size).map_or(u32::MAX, |i| i as u32);
    let out_pairs = pairs(out_ch);
    let main_sel = out_pairs.iter().position(|(_, pr)| *pr == (p.main_out_l, p.main_out_r)).map_or(u32::MAX, |i| i as u32);
    let ph_sel = out_pairs.iter().position(|(_, pr)| *pr == (p.phones_out_l, p.phones_out_r)).map_or(u32::MAX, |i| i as u32);
    let mix_opts = mix_inputs(in_ch);
    let mix_sel = mix_opts.iter().position(|(_, pr)| *pr == (p.mix_in_l, p.mix_in_r)).map_or(u32::MAX, |i| i as u32);
    let shared_pair = p.phones_routing && (p.main_out_l, p.main_out_r) == (p.phones_out_l, p.phones_out_r);
    let mix_on_guitar = p.mix_in_l == p.input_channel || p.mix_in_r == p.input_channel;

    let labels = |v: &[(String, (u32, u32))]| v.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>();
    let out_pair_labels = labels(&out_pairs);
    let mix_labels = labels(&mix_opts);
    let (inputs2, outputs2) = (inputs.clone(), outputs.clone());
    let (out_pairs_m, out_pairs_p) = (out_pairs.clone(), out_pairs.clone());

    rsx! {
        div {
            style: "flex: 1 1 0%; min-height: 0; display: flex; flex-direction: column; background: #09090b; color: #e4e4e7;",
            onkeydown: move |e: KeyboardEvent| if e.key() == Key::Escape { on_close.call(()) },
            // Header
            div { style: "display: flex; align-items: center; gap: 12px; padding: 14px 20px; border-bottom: 1px solid #27272a; flex-shrink: 0;",
                span { style: "font-size: 15px; font-weight: 700;", "Audio Settings" }
                span { style: "font-size: 11px; color: #71717a;", "⌘," }
                div { style: "flex: 1 1 0%;" }
                if dirty {
                    span { style: "font-size: 11px; color: #eab308;", "Unsaved — applying reopens the audio device (a short gap)" }
                }
                button {
                    style: "padding: 5px 12px; border-radius: 6px; border: 1px solid #3f3f46; background: transparent; color: #a1a1aa; font-size: 12px;",
                    onclick: {
                        let base = bridge.prefs.clone();
                        move |_| edit.set(base.clone())
                    },
                    "Revert"
                }
                button {
                    style: if dirty { "padding: 5px 14px; border-radius: 6px; border: none; background: #2563eb; color: #fff; font-size: 12px; font-weight: 600;" } else { "padding: 5px 14px; border-radius: 6px; border: none; background: #27272a; color: #71717a; font-size: 12px; font-weight: 600;" },
                    onclick: move |_| if dirty { on_save.call(edit()) },
                    "Save & apply"
                }
                button {
                    style: "padding: 5px 12px; border-radius: 6px; border: 1px solid #3f3f46; background: transparent; color: #e4e4e7; font-size: 12px;",
                    onclick: move |_| on_close.call(()),
                    "Done"
                }
            }
            // Body
            div { style: "flex: 1 1 0%; min-height: 0; overflow-y: scroll; padding: 18px 20px;",
                div { style: "display: flex; flex-wrap: wrap; gap: 16px; align-items: flex-start;",

                    Card { title: "Interface", note: "The device the rig plays through.",
                        Row { label: "Input device",
                            signal_widgets::Picker { options: in_devs, selected: in_sel, placeholder: device_label(&p.input_device), width: "240px".to_string(),
                                on_select: move |i: u32| {
                                    let name = if i == 0 { String::new() } else { inputs2.get(i as usize - 1).map(|d| d.name.clone()).unwrap_or_default() };
                                    edit.with_mut(|e| { e.input_device = name; e.input_channel = 0; });
                                },
                            }
                        }
                        Row { label: "Guitar input",
                            signal_widgets::Picker { options: guitar_in, selected: p.input_channel, width: "240px".to_string(),
                                on_select: move |i: u32| edit.with_mut(|e| e.input_channel = i),
                            }
                        }
                        Row { label: "Output device",
                            signal_widgets::Picker { options: out_devs, selected: out_sel, placeholder: device_label(&p.output_device), width: "240px".to_string(),
                                on_select: move |i: u32| {
                                    let name = if i == 0 { String::new() } else { outputs2.get(i as usize - 1).map(|d| d.name.clone()).unwrap_or_default() };
                                    edit.with_mut(|e| e.output_device = name);
                                },
                            }
                        }
                        Row { label: "Sample rate",
                            signal_widgets::Picker { options: rates, selected: rate_sel, placeholder: format!("{} Hz", p.sample_rate), width: "240px".to_string(),
                                on_select: move |i: u32| if let Some((hz, _)) = SAMPLE_RATES.get(i as usize) { edit.with_mut(|e| e.sample_rate = *hz) },
                            }
                        }
                        Row { label: "Buffer",
                            signal_widgets::Picker { options: bufs, selected: buf_sel, placeholder: format!("{} frames", p.buffer_size), width: "240px".to_string(),
                                on_select: move |i: u32| if let Some(b) = BUFFER_SIZES.get(i as usize) { edit.with_mut(|e| e.buffer_size = *b) },
                            }
                        }
                    }

                    Card { title: "Outputs", note: "Where the main mix (to the PA) and your phones go.",
                        Toggle {
                            label: "Separate phones outputs",
                            hint: "The phones get outputs of their own, so they can carry your mix while the PA gets only the rig. Off: everything on outputs 1-2.",
                            on: p.phones_routing,
                            on_change: move |v: bool| edit.with_mut(|e| e.phones_routing = v),
                        }
                        if p.phones_routing {
                            Row { label: "Main (to the PA)",
                                signal_widgets::Picker { options: out_pair_labels.clone(), selected: main_sel, placeholder: format!("{}-{}", p.main_out_l + 1, p.main_out_r + 1), width: "120px".to_string(),
                                    on_select: move |i: u32| if let Some((_, (l, r))) = out_pairs_m.get(i as usize) { edit.with_mut(|e| { e.main_out_l = *l; e.main_out_r = *r; }) },
                                }
                            }
                            Row { label: "Phones",
                                signal_widgets::Picker { options: out_pair_labels, selected: ph_sel, placeholder: format!("{}-{}", p.phones_out_l + 1, p.phones_out_r + 1), width: "120px".to_string(),
                                    on_select: move |i: u32| if let Some((_, (l, r))) = out_pairs_p.get(i as usize) { edit.with_mut(|e| { e.phones_out_l = *l; e.phones_out_r = *r; }) },
                                }
                            }
                            Hint { text: "On the MiniFuse, headphone output 1 plays outputs 1-2.".to_string() }
                            if shared_pair {
                                Warn { text: "Main and phones are the same pair — the mix would go to the PA too.".to_string() }
                            }
                        }
                        Row { label: "Main mute",
                            span { style: "font-size: 11px; color: #a1a1aa;",
                                if p.phones_routing { "mutes the PA only — the phones keep playing" } else { "mutes everything (one shared pair)" }
                            }
                        }
                    }

                    Card { title: "Headphone mix", note: "The band's monitor mix into two inputs, played in your phones by a process of its own — it keeps playing if the rig overruns, stops or crashes.",
                        Toggle {
                            label: "Headphone mixer",
                            hint: if p.phones_routing { "Plays the mix input below into the phones. The rig adds only your guitar.".to_string() } else { "Needs separate phones outputs (Outputs, left).".to_string() },
                            on: p.phones_mixer && p.phones_routing,
                            on_change: move |v: bool| edit.with_mut(|e| { e.phones_mixer = v; if v { e.phones_routing = true; } }),
                        }
                        Row { label: "Mix input",
                            signal_widgets::Picker { options: mix_labels, selected: mix_sel, placeholder: format!("{}-{}", p.mix_in_l + 1, p.mix_in_r + 1), width: "140px".to_string(),
                                on_select: move |i: u32| if let Some((_, (l, r))) = mix_opts.get(i as usize) { edit.with_mut(|e| { e.mix_in_l = *l; e.mix_in_r = *r; }) },
                            }
                        }
                        if mix_on_guitar {
                            Warn { text: "The mix input includes the guitar input.".to_string() }
                        }
                        if let Some(state) = state {
                            PhonesLive { state }
                        } else {
                            Hint { text: "Connect to the rig to set the phones levels.".to_string() }
                        }
                    }
                }
            }
        }
    }
}

/// The live half of the headphone card: the mixer's state, the incoming
/// mix's meter, and the three levels (they move as you drag — no save).
#[component]
fn PhonesLive(state: crate::state::RigViewState) -> Element {
    use signal_guitar_proto::rig::RigClient;
    use signal_guitar_proto::{PhonesMixerState, phones_fader_db};
    let rig = use_hook(try_consume_context::<RigClient>);
    let hp = state.perf.read().headphone.clone();
    let (ml, mr) = state.mix_db.cloned();
    let m = hp.mixer.clone();
    let (dot, text) = match (m.enabled, m.state) {
        (false, _) => ("#52525b", "Off".to_string()),
        (true, PhonesMixerState::PLAYING) => ("#22c55e", format!("Playing · {:.1} kHz · {} frames · pid {}", m.rate as f32 / 1000.0, m.block, m.pid)),
        (true, PhonesMixerState::NO_DEVICE) => ("#ef4444", "Running — the interface is not there, retrying".to_string()),
        (true, _) => ("#eab308", "Starting…".to_string()),
    };
    let db = |pos: f32| {
        let d = phones_fader_db(pos);
        if d.is_finite() { format!("{d:+.1} dB") } else { "off".to_string() }
    };
    let meter = |v: f32| ((v + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0);
    let (r1, r2, r3, r4) = (rig.clone(), rig.clone(), rig.clone(), rig);
    let (vol, gtr) = (hp.volume, hp.self_mix);
    rsx! {
        div { style: "display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-radius: 8px; background: #111113; border: 1px solid #27272a;",
            span { style: "width: 8px; height: 8px; border-radius: 999px; background: {dot}; flex-shrink: 0;" }
            span { style: "font-size: 12px; color: #d4d4d8; flex: 1 1 0%;", "Mixer: {text}" }
            if m.enabled {
                button {
                    style: "padding: 3px 10px; border-radius: 5px; border: 1px solid #3f3f46; background: transparent; color: #a1a1aa; font-size: 11px;",
                    title: "Restart the headphone mixer (a short gap in the mix)",
                    onclick: move |_| if let Some(r) = r1.clone() { spawn(async move { let _ = r.restart_phones_mixer().await; }); },
                    "Restart"
                }
            }
        }
        div { style: "display: flex; flex-direction: column; gap: 3px;",
            span { style: "font-size: 11px; color: #71717a;", "Mix in" }
            for (side, v) in [("L", ml), ("R", mr)] {
                div { key: "{side}", style: "display: flex; align-items: center; gap: 6px;",
                    span { style: "font-size: 10px; color: #71717a; width: 10px;", "{side}" }
                    div { style: "flex: 1 1 0%; height: 6px; background: #18181b; border-radius: 3px; overflow: hidden;",
                        div { style: "height: 100%; width: {meter(v)}%; background: #22c55e;" }
                    }
                    span { style: "font-size: 10px; color: #71717a; width: 44px; text-align: right;", if v > -89.0 { "{v:.0} dB" } else { "—" } }
                }
            }
        }
        LevelRow { label: "Mix", value: hp.mix_level, readout: db(hp.mix_level),
            on_change: move |v: f32| if let Some(r) = r2.clone() { spawn(async move { let _ = r.set_phones_mix(v).await; }); } }
        LevelRow { label: "Your guitar", value: gtr, readout: db(gtr),
            on_change: move |v: f32| if let Some(r) = r3.clone() { spawn(async move { let _ = r.set_headphone(vol, v).await; }); } }
        LevelRow { label: "Phones", value: vol, readout: db(vol),
            on_change: move |v: f32| if let Some(r) = r4.clone() { spawn(async move { let _ = r.set_headphone(v, gtr).await; }); } }
        Hint { text: "Unity is three-quarters up; the top is +12 dB. The same levels are on the Control view's phones strip.".to_string() }
    }
}

/// A horizontal level slider: drag or click anywhere on it; double-click
/// for unity.
#[component]
fn LevelRow(label: String, value: f32, readout: String, on_change: Callback<f32>) -> Element {
    let mut el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let pct = (value * 100.0).clamp(0.0, 100.0);
    let unity = signal_guitar_proto::PHONES_UNITY * 100.0;
    rsx! {
        div { style: "display: flex; align-items: center; gap: 10px;",
            span { style: "font-size: 12px; color: #a1a1aa; width: 80px; flex-shrink: 0;", "{label}" }
            div {
                style: "position: relative; flex: 1 1 0%; height: 18px; cursor: ew-resize;",
                onmounted: move |e| el.set(Some(e.data())),
                ondoubleclick: move |_| on_change.call(signal_guitar_proto::PHONES_UNITY),
                onpointerdown: move |e: PointerEvent| {
                    let x = e.client_coordinates().x;
                    let el = el();
                    let bus = signal_widgets::DragBus::try_use();
                    spawn(async move {
                        let Some(el) = el else { return };
                        let Ok(rect) = el.get_client_rect().await else { return };
                        let (left, w) = (rect.origin.x, rect.width().max(1.0));
                        on_change.call(((x - left) / w).clamp(0.0, 1.0) as f32);
                        if let Some(bus) = bus {
                            bus.begin(move |ev| {
                                if let signal_widgets::DragEvent::Move { x, .. } = ev {
                                    on_change.call(((x - left) / w).clamp(0.0, 1.0) as f32);
                                }
                            });
                        }
                    });
                },
                div { style: "position: absolute; left: 0; right: 0; top: 7px; height: 4px; border-radius: 2px; background: #27272a;" }
                div { style: "position: absolute; left: 0; width: {pct}%; top: 7px; height: 4px; border-radius: 2px; background: #3b82f6;" }
                div { style: "position: absolute; left: {unity}%; top: 3px; width: 1px; height: 12px; background: #52525b;" }
                div { style: "position: absolute; left: calc({pct}% - 6px); top: 3px; width: 12px; height: 12px; border-radius: 999px; background: #e4e4e7;" }
            }
            span { style: "font-size: 11px; color: #d4d4d8; width: 58px; text-align: right; font-family: monospace;", "{readout}" }
        }
    }
}

#[component]
fn Card(title: &'static str, note: &'static str, children: Element) -> Element {
    rsx! {
        div { style: "width: 440px; max-width: 100%; display: flex; flex-direction: column; gap: 12px; padding: 16px; border-radius: 10px; background: #0f0f11; border: 1px solid #27272a;",
            div { style: "display: flex; flex-direction: column; gap: 3px;",
                span { style: "font-size: 13px; font-weight: 700; color: #f4f4f5;", "{title}" }
                span { style: "font-size: 11px; color: #71717a; line-height: 1.4;", "{note}" }
            }
            {children}
        }
    }
}

/// A labelled row: label left, control right.
#[component]
fn Row(label: &'static str, children: Element) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 26px;",
            span { style: "font-size: 12px; color: #a1a1aa;", "{label}" }
            {children}
        }
    }
}

/// An on/off switch with what it does beneath.
#[component]
fn Toggle(label: &'static str, hint: String, on: bool, on_change: Callback<bool>) -> Element {
    let (track, knob) = if on { ("#2563eb", "18px") } else { ("#3f3f46", "2px") };
    rsx! {
        div { style: "display: flex; align-items: flex-start; gap: 10px; cursor: pointer;",
            onclick: move |_| on_change.call(!on),
            div { key: "{on}", style: "position: relative; width: 34px; height: 18px; border-radius: 999px; background: {track}; flex-shrink: 0; margin-top: 1px;",
                div { style: "position: absolute; top: 2px; left: {knob}; width: 14px; height: 14px; border-radius: 999px; background: #fff;" }
            }
            div { style: "display: flex; flex-direction: column; gap: 2px;",
                span { style: "font-size: 12px; font-weight: 600; color: #e4e4e7;", "{label}" }
                span { style: "font-size: 11px; color: #71717a; line-height: 1.4;", "{hint}" }
            }
        }
    }
}

#[component]
fn Hint(text: String) -> Element {
    rsx! { span { style: "font-size: 11px; color: #71717a; line-height: 1.4;", "{text}" } }
}

#[component]
fn Warn(text: String) -> Element {
    rsx! { span { style: "font-size: 11px; color: #f59e0b; line-height: 1.4;", "⚠ {text}" } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_four_channel_interface_offers_two_pairs_and_every_mono_input() {
        assert_eq!(pairs(4).iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["1-2", "3-4"]);
        let mix = mix_inputs(4);
        assert_eq!(mix[1], ("3-4 (stereo)".to_string(), (2, 3)));
        assert_eq!(mix[4], ("3 (mono)".to_string(), (2, 2)));
    }
}
