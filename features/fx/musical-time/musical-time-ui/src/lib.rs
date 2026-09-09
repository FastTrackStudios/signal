//! The control for a time that can be free-running or locked to the tempo.
//!
//! One control, shared by every face that has a time on it — the delay's two,
//! the reverb's pre-delay, the compressor's attack and release. They are the
//! same question each time ("milliseconds, or a note?") and there is nothing
//! plugin-specific about answering it.
//!
//! ## Why a stepper and not a list
//!
//! There are 21 note values once dotted and triplet are counted, which is too
//! many for a row of buttons and an awkward number for a knob — you would be
//! dragging through sixteenths to reach a half note. But the 21 are not a flat
//! list: they are 7 note values × 3 flavours. So the control has two gestures
//! matching the two axes — arrows step the note value, and a D/T pair sets the
//! flavour — and every one of the 21 is at most two clicks away.
//!
//! Inline styles only, per the repo's UI rules: Blitz does not load external
//! stylesheets, and these components are embedded in plugin editors.

use dioxus::prelude::*;
use fts_audio_ui::ParamHandle;
use musical_time::{Flavour, MusicalTime, NoteValue};

/// Read the note value a division parameter is currently on.
fn current(handle: &ParamHandle) -> MusicalTime {
    // `ParamHandle` speaks normalized; the division parameter is a stepped
    // integer over the whole table, so the normalized position maps straight
    // onto an index.
    let n = f64::from(handle.normalized()).clamp(0.0, 1.0);
    let last = (MusicalTime::COUNT - 1) as f64;
    MusicalTime::from_index((n * last).round() as usize)
}

/// Write a note value back to a division parameter, as one gesture.
fn set(handle: &ParamHandle, value: MusicalTime) {
    let last = (MusicalTime::COUNT - 1) as f32;
    // One gesture, so a host records the change as an edit rather than as a
    // value that moved on its own.
    handle.set_as_gesture(value.index() as f32 / last);
}

/// The note-value picker: `‹ 1/8 ›` with a `D` and a `T`.
///
/// `handle` is the division parameter built by
/// [`musical_time::params::division_param`].
#[component]
pub fn NotePicker(
    handle: ParamHandle,
    /// Prefix for the test ids, so a face with two of these can tell them
    /// apart: `{testid}-note`, `{testid}-dotted`, `{testid}-triplet`.
    testid: String,
    /// Text colour.
    #[props(default = "#e6e2d4".to_string())]
    ink: String,
    /// The colour a flavour button takes when it is the active one.
    #[props(default = "#43d17a".to_string())]
    accent: String,
    /// Off draws the picker dimmed and ignores clicks — for a side that is
    /// following another (a linked delay's right channel), where the control
    /// still has to SHOW what it is set to but changing it would do nothing.
    #[props(default = true)]
    enabled: bool,
    #[props(default = 1.0)] scale: f64,
) -> Element {
    let now = current(&handle);
    // A slaved side still shows its note; it just cannot be changed.
    let dim = if enabled { "1" } else { "0.45" };

    // Stepping the note value keeps the flavour: someone on 1/8T who wants
    // 1/16T is asking for the next note, not for straight sixteenths.
    let step = {
        let handle = handle.clone();
        move |delta: i32| {
            if !enabled {
                return;
            }
            let i = NoteValue::ALL
                .iter()
                .position(|v| *v == now.value)
                .unwrap_or(0) as i32;
            let next = (i + delta).clamp(0, NoteValue::ALL.len() as i32 - 1);
            set(
                &handle,
                MusicalTime::new(NoteValue::ALL[next as usize], now.flavour),
            );
        }
    };

    // A flavour button toggles: pressing the lit D returns to straight, which
    // is the only way back without cycling through T.
    let flavour = {
        let handle = handle.clone();
        move |f: Flavour| {
            if !enabled {
                return;
            }
            let next = if now.flavour == f {
                Flavour::Straight
            } else {
                f
            };
            set(&handle, MusicalTime::new(now.value, next));
        }
    };

    let px = |v: f64| format!("{:.1}px", v * scale);
    let arrow = format!(
        "border:none; background:transparent; cursor:pointer; padding:0; \
         color:{ink}; font-size:{}; line-height:1; opacity:0.75;",
        px(12.0)
    );
    let chip = |on: bool| {
        format!(
            "border:1px solid {}; border-radius:{}; cursor:pointer; \
             padding:{} {}; font-size:{}; line-height:1; font-weight:600; \
             background:{}; color:{};",
            if on {
                accent.clone()
            } else {
                "#00000033".to_string()
            },
            px(3.0),
            px(1.0),
            px(4.0),
            px(9.0),
            if on {
                accent.clone()
            } else {
                "transparent".to_string()
            },
            if on {
                "#101216".to_string()
            } else {
                ink.clone()
            },
        )
    };

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:{px(4.0)}; \
                    opacity:{dim};",

            button {
                "data-testid": "{testid}-prev",
                style: "{arrow}",
                onclick: {
                    let step = step.clone();
                    move |_| step(-1)
                },
                "‹"
            }
            div {
                "data-testid": "{testid}-note",
                style: "min-width:{px(30.0)}; text-align:center; color:{ink}; \
                        font-size:{px(11.0)}; font-weight:700; \
                        font-variant-numeric:tabular-nums;",
                "{now.value.label()}"
            }
            button {
                "data-testid": "{testid}-next",
                style: "{arrow}",
                onclick: {
                    let step = step.clone();
                    move |_| step(1)
                },
                "›"
            }

            button {
                "data-testid": "{testid}-dotted",
                style: "{chip(now.flavour == Flavour::Dotted)}",
                onclick: {
                    let flavour = flavour.clone();
                    move |_| flavour(Flavour::Dotted)
                },
                "D"
            }
            button {
                "data-testid": "{testid}-triplet",
                style: "{chip(now.flavour == Flavour::Triplet)}",
                onclick: {
                    let flavour = flavour.clone();
                    move |_| flavour(Flavour::Triplet)
                },
                "T"
            }
        }
    }
}

/// The unit switch: milliseconds, or a note.
///
/// It reads MS / NOTE rather than the FREE / SYNC most plugins use. Partly
/// because it says what the two modes actually are — one is a number of
/// milliseconds, the other is a note — and partly because it has to share a
/// panel with the delay's own macro controls, one of which the digital
/// profile legends "SYNC". Two things called Sync on one face is worse than a
/// slightly unusual name.
///
/// The switch is separate from [`NotePicker`] because the picker REPLACES the
/// knob it belongs to: while a time is locked to the tempo there is no
/// milliseconds value to enter, so offering a dial for one is offering a
/// control that does nothing.
#[component]
pub fn TimeModeSwitch(
    /// The mode parameter — [`musical_time::params::sync_param`].
    sync: ParamHandle,
    /// The host's tempo, if it has one. `None` greys NOTE out: a note value
    /// is not a duration without a tempo behind it, and a switch that flips
    /// and then silently does nothing is worse than one that will not flip.
    #[props(default)]
    tempo: Option<f64>,
    testid: String,
    #[props(default = "#e6e2d4".to_string())] ink: String,
    #[props(default = "#43d17a".to_string())] accent: String,
    #[props(default = 1.0)] scale: f64,
) -> Element {
    let synced = sync.normalized() >= 0.5;
    let has_tempo = tempo.is_some_and(|t| t > 0.0 && t.is_finite());
    let px = |v: f64| format!("{:.1}px", v * scale);

    let toggle = {
        let sync = sync.clone();
        move |want: bool| {
            if (sync.normalized() >= 0.5) == want {
                return;
            }
            sync.set_as_gesture(if want { 1.0 } else { 0.0 });
        }
    };

    let half = |on: bool, enabled: bool, accent: &str, ink: &str| {
        format!(
            "border:none; cursor:{}; padding:{} {}; font-size:{}; \
             line-height:1; font-weight:700; letter-spacing:0.04em; \
             background:{}; color:{}; opacity:{};",
            if enabled { "pointer" } else { "default" },
            px(2.0),
            px(6.0),
            px(9.0),
            if on { accent } else { "#00000022" },
            if on { "#101216" } else { ink },
            if enabled { "1" } else { "0.4" },
        )
    };
    let ms_style = half(!synced, true, &accent, &ink);
    let note_style = half(synced, has_tempo, &accent, &ink);

    rsx! {
        div {
            "data-testid": "{testid}",
            style: "display:flex; border-radius:{px(3.0)}; overflow:hidden; \
                    border:1px solid #00000033; width:fit-content;",
            button {
                "data-testid": "{testid}-ms",
                style: "{ms_style}",
                onclick: {
                    let toggle = toggle.clone();
                    move |_| toggle(false)
                },
                "MS"
            }
            button {
                "data-testid": "{testid}-note",
                style: "{note_style}",
                onclick: {
                    let toggle = toggle.clone();
                    move |_| {
                        // Refusing rather than switching into something that
                        // cannot work: with no tempo, NOTE would leave the
                        // control reading a note and sounding like the
                        // milliseconds, which looks like a bug.
                        if has_tempo {
                            toggle(true);
                        }
                    }
                },
                "NOTE"
            }
        }
    }
}

/// A delay time the way a face should print it: enough digits to dial with,
/// not so many that the column jitters.
#[must_use]
pub fn format_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2} s", ms / 1000.0)
    } else if ms >= 100.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{ms:.1} ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_print_with_a_unit_and_a_stable_number_of_digits() {
        assert_eq!(format_ms(7.25), "7.2 ms");
        assert_eq!(format_ms(375.0), "375 ms");
        assert_eq!(format_ms(1500.0), "1.50 s");
    }

    /// The normalized round trip the picker relies on: a stepped parameter's
    /// position has to land back on the same entry, or stepping would drift.
    #[test]
    fn every_note_survives_the_normalized_round_trip() {
        let last = (MusicalTime::COUNT - 1) as f64;
        for (i, t) in MusicalTime::ALL.iter().enumerate() {
            let normalized = i as f64 / last;
            let back = MusicalTime::from_index((normalized * last).round() as usize);
            assert_eq!(back, *t, "index {i} did not survive");
        }
    }
}
