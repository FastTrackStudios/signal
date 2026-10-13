use crate::prelude::*;

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::{Knob, KnobVariant};

// =============================================================================
// Data Types
// =============================================================================

/// Musical key for auto-tune.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum MusicalKey {
    #[default]
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl MusicalKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            MusicalKey::C => "C",
            MusicalKey::CSharp => "C#",
            MusicalKey::D => "D",
            MusicalKey::DSharp => "D#",
            MusicalKey::E => "E",
            MusicalKey::F => "F",
            MusicalKey::FSharp => "F#",
            MusicalKey::G => "G",
            MusicalKey::GSharp => "G#",
            MusicalKey::A => "A",
            MusicalKey::ASharp => "A#",
            MusicalKey::B => "B",
        }
    }

    /// All available keys.
    pub fn all() -> &'static [MusicalKey] {
        &[
            Self::C,
            Self::CSharp,
            Self::D,
            Self::DSharp,
            Self::E,
            Self::F,
            Self::FSharp,
            Self::G,
            Self::GSharp,
            Self::A,
            Self::ASharp,
            Self::B,
        ]
    }
}

/// Scale type for auto-tune.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum ScaleType {
    #[default]
    Chromatic,
    Major,
    Minor,
    Custom,
}

impl ScaleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScaleType::Chromatic => "Chromatic",
            ScaleType::Major => "Major",
            ScaleType::Minor => "Minor",
            ScaleType::Custom => "Custom",
        }
    }

    /// All available scale types.
    pub fn all() -> &'static [ScaleType] {
        &[Self::Chromatic, Self::Major, Self::Minor, Self::Custom]
    }
}

/// Parameters for the tuner/auto-tune effect.
#[derive(Clone, PartialEq, Debug)]
pub struct TunerParams {
    /// Correction speed (0-100%).
    pub speed: f32,
    /// Humanize amount (0-100%).
    pub humanize: f32,
    /// Musical key for correction.
    pub key: MusicalKey,
    /// Scale type for correction.
    pub scale: ScaleType,
    /// Formant shift in semitones (-12 to +12).
    pub formant: f32,
    /// Correction amount (0-100%).
    pub correction: f32,
}

impl Default for TunerParams {
    fn default() -> Self {
        Self {
            speed: 50.0,
            humanize: 0.0,
            key: MusicalKey::C,
            scale: ScaleType::Chromatic,
            formant: 0.0,
            correction: 100.0,
        }
    }
}

/// Real-time metering data for the tuner.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct TunerMetering {
    /// Detected pitch in Hz.
    pub detected_pitch: f32,
    /// Cents offset from nearest note (-50 to +50).
    pub cents_offset: f32,
    /// Current correction amount being applied (0-1).
    pub correction_amount: f32,
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get the note name from MIDI note number.
fn midi_to_note(midi: i32) -> String {
    let note_names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let note = (midi % 12) as usize;
    let octave = (midi / 12) - 1;
    format!("{}{}", note_names[note], octave)
}

/// Convert frequency to MIDI note number.
fn freq_to_midi(freq: f32) -> f32 {
    69.0 + 12.0 * (freq / 440.0).log2()
}

/// Get the nearest MIDI note and cents offset.
fn get_nearest_note(freq: f32) -> (i32, f32) {
    if freq <= 0.0 {
        return (0, 0.0);
    }
    let midi_float = freq_to_midi(freq);
    let nearest_midi = midi_float.round() as i32;
    let cents = (midi_float - nearest_midi as f32) * 100.0;
    (nearest_midi, cents)
}

// =============================================================================
// TunerGraph Component
// =============================================================================

/// Props for the TunerGraph component.
#[derive(Props, Clone, PartialEq)]
pub struct TunerGraphProps {
    /// Signal for tuner parameters (allows two-way binding).
    pub params: Signal<TunerParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: TunerMetering,
    /// Size of the graph in pixels.
    #[props(default = 200)]
    pub graph_size: u32,
    /// Whether to show the knob controls panel.
    #[props(default = true)]
    pub show_controls: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

/// Tuner graph component with arc/needle visualization.
///
/// Renders a tuner interface with:
/// - Arc/needle style cents deviation meter (-50 to +50 cents)
/// - Current note indicator
/// - Speed, Humanize, Correction controls on the left
/// - Formant, Key, Scale controls on the right
#[component]
pub fn TunerGraph(props: TunerGraphProps) -> Element {
    let mut params = props.params;

    // Create local signals bound to params
    let mut speed_sig = use_signal(|| params.read().speed);
    let mut humanize_sig = use_signal(|| params.read().humanize);
    let mut correction_sig = use_signal(|| params.read().correction);
    let mut formant_sig = use_signal(|| params.read().formant);
    let mut key_sig = use_signal(|| params.read().key);
    let mut scale_sig = use_signal(|| params.read().scale);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        speed_sig.set(params_clone.speed);
        humanize_sig.set(params_clone.humanize);
        correction_sig.set(params_clone.correction);
        formant_sig.set(params_clone.formant);
        key_sig.set(params_clone.key);
        scale_sig.set(params_clone.scale);
    });

    // Graph dimensions
    let size = props.graph_size as f64;
    let center_x = size / 2.0;
    let center_y = size / 2.0;
    let arc_radius = size * 0.35;
    let needle_length = arc_radius * 0.8;

    // Arc angle range: -120° to +120° (240° total)
    let start_angle = -120.0_f64.to_radians();
    let end_angle = 120.0_f64.to_radians();
    let total_angle = end_angle - start_angle;

    // Current cents offset (-50 to +50)
    let cents = props.metering.cents_offset.clamp(-50.0, 50.0);
    let cents_normalized = (cents + 50.0) / 100.0; // 0 to 1
    let needle_angle = start_angle + (cents_normalized as f64 * total_angle);

    // Needle endpoint
    let needle_x = center_x + needle_length * needle_angle.cos();
    let needle_y = center_y + needle_length * needle_angle.sin();

    // Get detected note
    let (midi_note, _cents) = get_nearest_note(props.metering.detected_pitch);
    let note_name = if props.metering.detected_pitch > 0.0 {
        midi_to_note(midi_note)
    } else {
        "--".to_string()
    };

    // Arc path (outer arc)
    let arc_path = {
        let start_x = center_x + arc_radius * start_angle.cos();
        let start_y = center_y + arc_radius * start_angle.sin();
        let end_x = center_x + arc_radius * end_angle.cos();
        let end_y = center_y + arc_radius * end_angle.sin();
        format!(
            "M {:.1} {:.1} A {:.1} {:.1} 0 1 1 {:.1} {:.1}",
            start_x, start_y, arc_radius, arc_radius, end_x, end_y
        )
    };

    // Center zone arc (green zone, -5 to +5 cents)
    let center_start_angle = start_angle + (45.0 / 100.0) * total_angle;
    let center_end_angle = start_angle + (55.0 / 100.0) * total_angle;
    let center_arc_path = {
        let start_x = center_x + arc_radius * center_start_angle.cos();
        let start_y = center_y + arc_radius * center_start_angle.sin();
        let end_x = center_x + arc_radius * center_end_angle.cos();
        let end_y = center_y + arc_radius * center_end_angle.sin();
        format!(
            "M {:.1} {:.1} A {:.1} {:.1} 0 0 1 {:.1} {:.1}",
            start_x, start_y, arc_radius, arc_radius, end_x, end_y
        )
    };

    // Tick marks at -50, -25, 0, +25, +50 cents
    let tick_marks = [-50, -25, 0, 25, 50]
        .iter()
        .map(|&cent| {
            let norm = (cent + 50) as f64 / 100.0;
            let angle = start_angle + norm * total_angle;
            let inner_r = arc_radius - 8.0;
            let outer_r = arc_radius + 4.0;
            let x1 = center_x + inner_r * angle.cos();
            let y1 = center_y + inner_r * angle.sin();
            let x2 = center_x + outer_r * angle.cos();
            let y2 = center_y + outer_r * angle.sin();
            (x1, y1, x2, y2, cent)
        })
        .collect::<Vec<_>>();

    // Theme colors - Cyan
    let arc_color = "rgba(6, 182, 212, 0.3)";
    let center_zone_color = "#10b981"; // Green for in-tune zone
    let needle_color = "#06b6d4"; // Cyan
    let tick_color = "rgba(255, 255, 255, 0.3)";
    let text_color = "rgba(255, 255, 255, 0.6)";
    let note_color = "#06b6d4";

    // Value formatters
    let format_percent = |v: f32| format!("{v:.0}%");
    let format_semitones = |v: f32| {
        if v >= 0.0 {
            format!("+{v:.0}st")
        } else {
            format!("{v:.0}st")
        }
    };

    // Knob sizes
    let large_knob = 56_u32;
    let medium_knob = 44_u32;
    let small_knob = 36_u32;

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "tuner-graph flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #0f172a 0%, #1e293b 100%); border-radius: 8px; padding: 12px;",

                // Left column: Speed, Humanize, Correction
                if props.show_controls {
                    div {
                        class: "flex flex-col items-center gap-2",
                        style: "min-width: 70px;",

                        Knob {
                            value: speed_sig,
                            min: 0.0,
                            max: 100.0,
                            size: large_knob,
                            label: Some("SPEED".to_string()),
                            value_display: Some(format_percent(speed_sig())),
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                speed_sig.set(v);
                                params.write().speed = v;
                            },
                        }

                        Knob {
                            value: humanize_sig,
                            min: 0.0,
                            max: 100.0,
                            size: medium_knob,
                            label: Some("HUMAN".to_string()),
                            value_display: Some(format_percent(humanize_sig())),
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                humanize_sig.set(v);
                                params.write().humanize = v;
                            },
                        }

                        Knob {
                            value: correction_sig,
                            min: 0.0,
                            max: 100.0,
                            size: medium_knob,
                            label: Some("CORR".to_string()),
                            value_display: Some(format_percent(correction_sig())),
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                correction_sig.set(v);
                                params.write().correction = v;
                            },
                        }
                    }
                }

                // Center: Arc/Needle graph
                div {
                    class: "tuner-graph-svg",
                    style: "width: {props.graph_size}px; height: {props.graph_size}px;",

                    svg {
                        width: "100%",
                        height: "100%",
                        view_box: "0 0 {size} {size}",
                        preserve_aspect_ratio: "xMidYMid meet",
                        style: "background: #080c14; border-radius: 6px;",

                        // Background circle
                        circle {
                            cx: "{center_x}",
                            cy: "{center_y}",
                            r: "{arc_radius + 20.0}",
                            fill: "#050810",
                        }

                        // Arc background
                        path {
                            d: "{arc_path}",
                            stroke: "{arc_color}",
                            stroke_width: "8",
                            stroke_linecap: "round",
                            fill: "none",
                        }

                        // Center zone (green, in-tune)
                        path {
                            d: "{center_arc_path}",
                            stroke: "{center_zone_color}",
                            stroke_width: "8",
                            stroke_linecap: "round",
                            fill: "none",
                        }

                        // Tick marks
                        for (x1, y1, x2, y2, cent) in tick_marks.iter() {
                            line {
                                x1: "{x1}",
                                y1: "{y1}",
                                x2: "{x2}",
                                y2: "{y2}",
                                stroke: "{tick_color}",
                                stroke_width: if *cent == 0 { "2" } else { "1" },
                            }
                        }

                        // Tick labels
                        for (x1, y1, x2, y2, cent) in tick_marks.iter() {
                            text {
                                x: "{x2 + (x2 - center_x) * 0.15}",
                                y: "{y2 + (y2 - center_y) * 0.15 + 3.0}",
                                text_anchor: "middle",
                                fill: "{text_color}",
                                font_size: "9",
                                font_family: "system-ui",
                                "{cent}"
                            }
                        }

                        // Needle
                        line {
                            x1: "{center_x}",
                            y1: "{center_y}",
                            x2: "{needle_x}",
                            y2: "{needle_y}",
                            stroke: "{needle_color}",
                            stroke_width: "3",
                            stroke_linecap: "round",
                        }

                        // Needle tip
                        circle {
                            cx: "{needle_x}",
                            cy: "{needle_y}",
                            r: "4",
                            fill: "{needle_color}",
                            stroke: "#fff",
                            stroke_width: "1.5",
                        }

                        // Center pivot
                        circle {
                            cx: "{center_x}",
                            cy: "{center_y}",
                            r: "6",
                            fill: "#1e293b",
                            stroke: "{needle_color}",
                            stroke_width: "2",
                        }

                        // Note name display (below center)
                        text {
                            x: "{center_x}",
                            y: "{center_y + 30.0}",
                            text_anchor: "middle",
                            fill: "{note_color}",
                            font_size: "24",
                            font_weight: "600",
                            font_family: "system-ui",
                            "{note_name}"
                        }

                        // Frequency display (below note)
                        if props.metering.detected_pitch > 0.0 {
                            text {
                                x: "{center_x}",
                                y: "{center_y + 48.0}",
                                text_anchor: "middle",
                                fill: "{text_color}",
                                font_size: "10",
                                font_family: "system-ui",
                                "{props.metering.detected_pitch:.1} Hz"
                            }
                        }

                        // Cents display (above center)
                        text {
                            x: "{center_x}",
                            y: "{center_y - 20.0}",
                            text_anchor: "middle",
                            fill: "{text_color}",
                            font_size: "12",
                            font_family: "system-ui",
                            "{cents:+.0} ¢"
                        }
                    }
                }

                // Right column: Formant, Key, Scale
                if props.show_controls {
                    div {
                        class: "flex flex-col items-center gap-2",
                        style: "min-width: 70px;",

                        Knob {
                            value: formant_sig,
                            min: -12.0,
                            max: 12.0,
                            size: large_knob,
                            label: Some("FORM".to_string()),
                            value_display: Some(format_semitones(formant_sig())),
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                formant_sig.set(v);
                                params.write().formant = v;
                            },
                        }

                        // Key selector (as a small display, click to cycle)
                        div {
                            class: "flex flex-col items-center gap-1 cursor-pointer",
                            onclick: move |_| {
                                if !props.interactive {
                                    return;
                                }
                                let keys = MusicalKey::all();
                                let current_idx = keys.iter().position(|k| k == &key_sig()).unwrap_or(0);
                                let next_idx = (current_idx + 1) % keys.len();
                                key_sig.set(keys[next_idx]);
                                params.write().key = keys[next_idx];
                            },

                            div {
                                class: "text-[9px] text-gray-500 leading-none mb-1",
                                "KEY"
                            }
                            div {
                                class: "flex items-center justify-center w-11 h-11 rounded-full bg-gray-800 border border-cyan-600 text-cyan-400",
                                style: "font-size: 16px; font-weight: 600;",
                                "{key_sig().as_str()}"
                            }
                        }

                        // Scale selector (as a small display, click to cycle)
                        div {
                            class: "flex flex-col items-center gap-1 cursor-pointer",
                            onclick: move |_| {
                                if !props.interactive {
                                    return;
                                }
                                let scales = ScaleType::all();
                                let current_idx = scales.iter().position(|s| s == &scale_sig()).unwrap_or(0);
                                let next_idx = (current_idx + 1) % scales.len();
                                scale_sig.set(scales[next_idx]);
                                params.write().scale = scales[next_idx];
                            },

                            div {
                                class: "text-[9px] text-gray-500 leading-none mb-1",
                                "SCALE"
                            }
                            div {
                                class: "flex items-center justify-center w-11 h-11 rounded bg-gray-800 border border-cyan-600 text-cyan-400 text-center px-1",
                                style: "font-size: 8px; font-weight: 600; line-height: 1.1;",
                                "{scale_sig().as_str()}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// TunerWidget - Full widget with integrated controls
// =============================================================================

/// Props for the TunerWidget component (full widget with all controls).
#[derive(Props, Clone, PartialEq)]
pub struct TunerWidgetProps {
    /// Signal for tuner parameters (allows two-way binding).
    pub params: Signal<TunerParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: TunerMetering,
    /// Size of the graph in pixels.
    #[props(default = 200)]
    pub graph_size: u32,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

/// Full tuner widget with all controls.
#[component]
pub fn TunerWidget(props: TunerWidgetProps) -> Element {
    rsx! {
        div {
            class: "tuner-widget",

            TunerGraph {
                params: props.params,
                metering: props.metering.clone(),
                graph_size: props.graph_size,
                show_controls: true,
                interactive: props.interactive,
            }
        }
    }
}
