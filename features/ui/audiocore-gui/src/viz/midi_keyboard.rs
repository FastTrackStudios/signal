use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// Interactive horizontal piano keyboard display.
///
/// Renders a standard piano layout with white and black keys.
/// Active notes are highlighted with the accent color.
#[component]
pub fn MidiKeyboard(
    active_notes: Vec<u8>,
    #[props(default = 2)] start_octave: u8,
    #[props(default = 4)] num_octaves: u8,
    #[props(default = 60)] height: u32,
    #[props(default)] on_note_click: Option<EventHandler<u8>>,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    // Standard piano layout: 7 white keys per octave
    // Black keys at positions: C#, D#, F#, G#, A#
    let white_key_width = 20u32; // px per white key
    let total_white_keys = num_octaves as u32 * 7;
    let total_width = total_white_keys * white_key_width;
    let black_key_height = (height as f32 * 0.6) as u32;
    let black_key_width = 14u32;

    // Map of which semitones are white keys: C=0, D=2, E=4, F=5, G=7, A=9, B=11
    let white_semitones = [0u8, 2, 4, 5, 7, 9, 11];
    // Black keys and their offsets relative to the white key they sit between
    // C#=1, D#=3, F#=6, G#=8, A#=10
    // Black key x-offsets relative to octave start (in white key units)
    // C#: between C(0) and D(1), D#: between D(1) and E(2)
    // F#: between F(3) and G(4), G#: between G(4) and A(5), A#: between A(5) and B(6)
    let black_offsets: [(u8, f32); 5] = [
        (1, 0.7),  // C# after C
        (3, 1.7),  // D# after D
        (6, 3.7),  // F# after F
        (8, 4.7),  // G# after G
        (10, 5.7), // A# after A
    ];

    rsx! {
        div {
            style: format!(
                "width:{total_width}px; height:{height}px; position:relative; \
                 {INSET} overflow:hidden;",
                INSET = t.style_inset(),
            ),

            // White keys
            for octave in 0..num_octaves {
                for (white_idx, &semitone) in white_semitones.iter().enumerate() {
                    {
                        let midi_note = (start_octave + octave) * 12 + semitone;
                        let is_active = active_notes.contains(&midi_note);
                        let x = (octave as u32 * 7 + white_idx as u32) * white_key_width;
                        let bg = if is_active { t.accent_dim } else { "#e8e8e8" };

                        rsx! {
                            div {
                                key: "w-{midi_note}",
                                style: format!(
                                    "position:absolute; left:{x}px; top:0; \
                                     width:{w}px; height:{height}px; \
                                     background:{bg}; \
                                     border-right:1px solid {BORDER}; \
                                     border-bottom:1px solid {BORDER}; \
                                     cursor:pointer; z-index:1;",
                                    w = white_key_width,
                                    BORDER = t.border,
                                ),
                                onclick: move |_| {
                                    if let Some(ref cb) = on_note_click {
                                        cb.call(midi_note);
                                    }
                                },
                            }
                        }
                    }
                }
            }

            // Black keys
            for octave in 0..num_octaves {
                for &(semitone, offset) in black_offsets.iter() {
                    {
                        let midi_note = (start_octave + octave) * 12 + semitone;
                        let is_active = active_notes.contains(&midi_note);
                        let x = (octave as u32 as f32 * 7.0 + offset) * white_key_width as f32
                            - (black_key_width as f32 / 2.0);
                        let bg = if is_active { t.accent } else { "#1a1a1e" };

                        rsx! {
                            div {
                                key: "b-{midi_note}",
                                style: format!(
                                    "position:absolute; left:{x}px; top:0; \
                                     width:{bw}px; height:{bh}px; \
                                     background:{bg}; \
                                     border-radius:0 0 {RADIUS} {RADIUS}; \
                                     cursor:pointer; z-index:2; \
                                     box-shadow:{SHADOW};",
                                    bw = black_key_width,
                                    bh = black_key_height,
                                    RADIUS = t.radius_small,
                                    SHADOW = t.shadow_subtle,
                                ),
                                onclick: move |_| {
                                    if let Some(ref cb) = on_note_click {
                                        cb.call(midi_note);
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
