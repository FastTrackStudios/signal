//! Audio settings — device / channel / buffer / sample-rate picker.
//!
//! Pure presentation over the proto types: the hosting view fetches devices
//! and prefs over the `AudioSettings` service, builds an [`AudioSettingsBridge`]
//! carrying them plus a save callback, and hands it to the modal.

use dioxus::prelude::*;
use lumen_blocks::components::button::{Button, ButtonVariant};
use lumen_blocks::components::dropdown::{Dropdown, DropdownContent, DropdownItem, DropdownTrigger};

use signal_guitar_proto::{AudioDevice, AudioPrefs};

/// Devices + prefs + save callback handed to the settings modal / pickers.
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

/// Selectable buffer sizes (frames) — matches the guitar TUI's `BUFFERS`.
const BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];

/// Selectable sample rates (Hz). `0` = device native.
const SAMPLE_RATES: &[(u32, &str)] = &[
    (0, "Device native"),
    (44_100, "44.1 kHz"),
    (48_000, "48 kHz"),
    (88_200, "88.2 kHz"),
    (96_000, "96 kHz"),
];

/// Max input channels to offer when the selected device's channel count is
/// unknown (e.g. the "system default" entry).
const DEFAULT_MAX_CHANNELS: u16 = 8;

/// Human label for a device value (empty string → "System default").
fn device_label(name: &str) -> String {
    if name.is_empty() {
        "System default".to_string()
    } else {
        name.to_string()
    }
}

/// Modal audio-settings page. Edits a local copy of the prefs and persists via
/// the bridge's `on_save` when the user clicks Save.
#[component]
pub fn AudioSettingsModal(bridge: AudioSettingsBridge, on_close: EventHandler<()>) -> Element {
    // Local edit state, seeded from the persisted prefs.
    let mut input_device = use_signal(|| bridge.prefs.input_device.clone());
    let mut input_channel = use_signal(|| bridge.prefs.input_channel as usize);
    let mut output_device = use_signal(|| bridge.prefs.output_device.clone());
    let mut sample_rate = use_signal(|| bridge.prefs.sample_rate);
    let mut buffer_size = use_signal(|| bridge.prefs.buffer_size);

    // Channel count for the currently-selected input device.
    let sel_input = input_device();
    let max_channels = bridge
        .inputs
        .iter()
        .find(|d| d.name == sel_input)
        .map(|d| d.channels.max(1))
        .unwrap_or(DEFAULT_MAX_CHANNELS);

    let inputs = bridge.inputs.clone();
    let outputs = bridge.outputs.clone();
    let on_save = bridge.on_save;

    let sample_rate_label = SAMPLE_RATES
        .iter()
        .find(|(hz, _)| *hz == sample_rate())
        .map(|(_, l)| l.to_string())
        .unwrap_or_else(|| format!("{} Hz", sample_rate()));

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| on_close.call(()),

            // Panel
            div {
                class: "w-[460px] max-w-[92vw] rounded-lg border border-border bg-popover text-popover-foreground shadow-2xl",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "flex items-center justify-between px-5 py-3 border-b border-border",
                    h2 { class: "text-sm font-semibold", "Audio Settings" }
                    Button {
                        variant: ButtonVariant::Ghost,
                        is_icon_button: true,
                        aria_label: "Close".to_string(),
                        on_click: move |_| on_close.call(()),
                        "×"
                    }
                }

                // Body
                div { class: "px-5 py-4 space-y-3",

                    // Input device
                    SettingRow { label: "Input device",
                        Dropdown {
                            DropdownTrigger {
                                Button { variant: ButtonVariant::Outline, full_width: true,
                                    span { class: "truncate", "{device_label(&sel_input)}" }
                                }
                            }
                            DropdownContent { width: "w-56".to_string(), class: "max-h-72 overflow-y-auto",
                                DropdownItem {
                                    value: String::new(),
                                    index: 0,
                                    on_select: move |v: String| { input_device.set(v); input_channel.set(0); },
                                    "System default"
                                }
                                for (i, d) in inputs.iter().enumerate() {
                                    DropdownItem {
                                        key: "{i}",
                                        value: d.name.clone(),
                                        index: i + 1,
                                        on_select: move |v: String| { input_device.set(v); input_channel.set(0); },
                                        "{d.name} ({d.channels} ch)"
                                    }
                                }
                            }
                        }
                    }

                    // Input channel
                    SettingRow { label: "Input channel",
                        Dropdown {
                            DropdownTrigger {
                                Button { variant: ButtonVariant::Outline, full_width: true,
                                    "Channel {input_channel() + 1}"
                                }
                            }
                            DropdownContent { width: "w-56".to_string(), class: "max-h-72 overflow-y-auto",
                                for ch in 0..max_channels as usize {
                                    DropdownItem {
                                        key: "{ch}",
                                        value: ch,
                                        index: ch,
                                        on_select: move |v: usize| input_channel.set(v),
                                        "Channel {ch + 1}"
                                    }
                                }
                            }
                        }
                    }

                    // Output device
                    SettingRow { label: "Output device",
                        Dropdown {
                            DropdownTrigger {
                                Button { variant: ButtonVariant::Outline, full_width: true,
                                    span { class: "truncate", "{device_label(&output_device())}" }
                                }
                            }
                            DropdownContent { width: "w-56".to_string(), class: "max-h-72 overflow-y-auto",
                                DropdownItem {
                                    value: String::new(),
                                    index: 0,
                                    on_select: move |v: String| output_device.set(v),
                                    "System default"
                                }
                                for (i, d) in outputs.iter().enumerate() {
                                    DropdownItem {
                                        key: "{i}",
                                        value: d.name.clone(),
                                        index: i + 1,
                                        on_select: move |v: String| output_device.set(v),
                                        "{d.name} ({d.channels} ch)"
                                    }
                                }
                            }
                        }
                    }

                    // Sample rate
                    SettingRow { label: "Sample rate",
                        Dropdown {
                            DropdownTrigger {
                                Button { variant: ButtonVariant::Outline, full_width: true, "{sample_rate_label}" }
                            }
                            DropdownContent { width: "w-56".to_string(),
                                for (idx, (hz, label)) in SAMPLE_RATES.iter().copied().enumerate() {
                                    DropdownItem {
                                        key: "{hz}",
                                        value: hz,
                                        index: idx,
                                        on_select: move |v: u32| sample_rate.set(v),
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }

                    // Buffer size
                    SettingRow { label: "Buffer size",
                        Dropdown {
                            DropdownTrigger {
                                Button { variant: ButtonVariant::Outline, full_width: true, "{buffer_size()} frames" }
                            }
                            DropdownContent { width: "w-56".to_string(),
                                for (idx, frames) in BUFFER_SIZES.iter().copied().enumerate() {
                                    DropdownItem {
                                        key: "{frames}",
                                        value: frames,
                                        index: idx,
                                        on_select: move |v: u32| buffer_size.set(v),
                                        "{frames} frames"
                                    }
                                }
                            }
                        }
                    }
                }

                // Footer
                div { class: "flex items-center justify-end gap-2 px-5 py-3 border-t border-border",
                    Button {
                        variant: ButtonVariant::Ghost,
                        on_click: move |_| on_close.call(()),
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        on_click: move |_| {
                            let prefs = AudioPrefs {
                                input_device: input_device(),
                                input_channel: input_channel() as u32,
                                output_device: output_device(),
                                sample_rate: sample_rate(),
                                buffer_size: buffer_size(),
                            };
                            on_save.call(prefs);
                            on_close.call(());
                        },
                        "Save"
                    }
                }
            }
        }
    }
}

/// A labelled settings row: label on the left, control on the right.
#[component]
fn SettingRow(label: String, children: Element) -> Element {
    rsx! {
        div { class: "flex items-center justify-between gap-4",
            label { class: "text-sm text-muted-foreground", "{label}" }
            div { class: "w-56", {children} }
        }
    }
}
