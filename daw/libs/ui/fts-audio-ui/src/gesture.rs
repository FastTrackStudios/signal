//! The one gesture layer every value control goes through.
//!
//! Spec: `docs/spec/fx/controls.md` (`fx.control.*`). A widget says *what*
//! it edits — a [`ParamHandle`], a [`DragAxis`], a sensitivity — and calls
//! these helpers from its event handlers. It never decides for itself what a
//! modifier means, what resets, or how a wheel notch maps to a value; two
//! widgets disagreeing on that is the defect this module exists to prevent
//! (`primitives.drift-is-a-bug`).
//!
//! | gesture | helper |
//! |---|---|
//! | pointer-down on the body | [`press`] — Alt-click resets, right-click reports a menu, else starts the drag |
//! | double-click on the body | [`double_click`] — reset to default |
//! | wheel | [`wheel`] — coarse / fine / ultra-fine step, one step for stepped params |
//! | key on a focused control | [`key`] — arrows, PgUp/PgDn, Home/End, Backspace/Delete, Enter |
//! | typed text | [`parse_typed`] — `1k`, `A4`, `2x`, `50%`, unit-optional |
//!
//! The live drag itself (capture, modifier ratio, re-anchoring, detent) lives
//! in [`crate::drag::DragProvider`], which reads [`fine_multiplier`] from here.

use crate::drag::{begin_drag_axis, DragAxis, DragState};
use crate::param::ParamHandle;
use dioxus::prelude::*;
use dioxus_elements::input_data::MouseButton;

// ── Constants ─────────────────────────────────────────────────────────────

/// Pixels of drag for a full 0→1 sweep on a knob.
// r[impl fx.control.drag.sensitivity]
pub const KNOB_SENSITIVITY: f64 = 150.0;
/// Pixels of drag for a full sweep on a linear slider.
pub const SLIDER_SENSITIVITY: f64 = 200.0;

/// Fine modifier ratio — Ctrl/Cmd (REAPER, Pro Tools) and Shift (FabFilter,
/// Logic) both give it.
// r[impl fx.control.fine]
pub const FINE: f64 = 8.0;
/// Ctrl+Shift together.
pub const ULTRA_FINE: f64 = 32.0;

/// Coarse wheel step, normalized (≈2 % of range).
// r[impl fx.control.wheel]
pub const WHEEL_STEP: f64 = 0.02;
/// Keyboard Page Up / Page Down step.
pub const PAGE_STEP: f64 = 0.10;

/// Width of the soft detent at a bipolar parameter's default, in pixels of
/// drag (`fx.control.bipolar`).
pub const DETENT_PX: f64 = 6.0;

// ── Modifiers ─────────────────────────────────────────────────────────────

/// The drag / wheel ratio for a modifier state: 1, [`FINE`] or [`ULTRA_FINE`].
// r[impl fx.control.fine]
pub fn fine_multiplier(mods: Modifiers) -> f64 {
    let ctrl = mods.ctrl() || mods.meta();
    let shift = mods.shift();
    if ctrl && shift {
        ULTRA_FINE
    } else if ctrl || shift {
        FINE
    } else {
        1.0
    }
}

/// Whether the modifier state asks for a fine gesture at all.
pub fn is_fine(mods: Modifiers) -> bool {
    fine_multiplier(mods) > 1.0
}

/// Normalized step for a wheel notch / arrow key under these modifiers.
pub fn wheel_step(mods: Modifiers) -> f64 {
    WHEEL_STEP / fine_multiplier(mods)
}

// ── Pointer-down ──────────────────────────────────────────────────────────

/// What a pointer-down on a control's body turned into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// A drag was started on the handle.
    Drag,
    /// Alt-click: the parameter was reset to its default.
    Reset,
    /// Secondary button: the widget should show its context menu (or, if it
    /// has no readout, open text entry). Nothing was changed.
    Menu,
    /// The control is disabled / inert: nothing happened.
    Ignored,
}

/// Handle a pointer-down on a control body.
///
/// Alt-click resets (`fx.control.reset`), right-click is reported as
/// [`Press::Menu`] and never starts a drag (`fx.control.context-menu`),
/// anything else begins a drag on `handle` along `axis` with the given
/// sensitivity (`fx.control.capture`).
// r[impl fx.control.reset]
// r[impl fx.control.context-menu]
// r[impl fx.control.capture]
pub fn press(
    evt: &MouseEvent,
    drag: &mut Signal<DragState>,
    handle: &ParamHandle,
    axis: DragAxis,
    sensitivity: f64,
) -> Press {
    if evt.trigger_button() == Some(MouseButton::Secondary) {
        evt.prevent_default();
        return Press::Menu;
    }
    if evt.modifiers().alt() {
        evt.prevent_default();
        handle.reset_to_default();
        return Press::Reset;
    }
    let p = evt.client_coordinates();
    begin_drag_axis(drag, handle.clone(), axis, p.x, p.y, sensitivity);
    Press::Drag
}

/// Vertical-axis shorthand for [`press`] (knobs, levers drawn vertically).
pub fn press_vertical(
    evt: &MouseEvent,
    drag: &mut Signal<DragState>,
    handle: &ParamHandle,
    sensitivity: f64,
) -> Press {
    press(evt, drag, handle, DragAxis::Vertical, sensitivity)
}

/// Double-click on a control body: reset to the parameter's default as one
/// edit gesture. Any drag the second press started is abandoned first so the
/// reset is the gesture the host records.
// r[impl fx.control.reset]
pub fn double_click(drag: &mut Signal<DragState>, handle: &ParamHandle) {
    if drag.read().active {
        drag.set(DragState::default());
    }
    handle.reset_to_default();
}

// ── Wheel ─────────────────────────────────────────────────────────────────

/// Wheel over a control: one coarse step per notch (fine / ultra-fine with
/// the modifiers), exactly one step for a stepped parameter, wheel-up
/// increases. Each notch is its own edit gesture.
// r[impl fx.control.wheel]
// r[impl fx.control.stepped]
pub fn wheel(evt: &WheelEvent, handle: &ParamHandle) {
    evt.prevent_default();
    let dy = evt.delta().strip_units().y;
    if dy == 0.0 {
        return;
    }
    let direction = if dy < 0.0 { 1.0 } else { -1.0 };
    nudge(handle, direction, wheel_step(evt.modifiers()) as f32);
}

/// Move `handle` one notch: a stepped parameter snaps one step, a continuous
/// one moves by `step` (normalized). One edit gesture.
pub fn nudge(handle: &ParamHandle, direction: f32, step: f32) {
    let cur = handle.normalized();
    let next = handle.stepped_from(cur, direction, step);
    if next != cur {
        handle.set_as_gesture(next);
    }
}

// ── Keyboard ──────────────────────────────────────────────────────────────

/// What a key press on a focused control asked for beyond a value change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// The key was not one of ours.
    Unhandled,
    /// The value was changed (or reset).
    Edited,
    /// Enter: the widget should open text entry.
    OpenTextEntry,
}

/// Keyboard on a focused control: ↑/→ increase, ↓/← decrease (wheel step,
/// fine with the modifier), Page Up/Down ±10 %, Home/End min/max,
/// Backspace/Delete reset, Enter opens text entry.
// r[impl fx.control.keyboard]
pub fn key(evt: &KeyboardEvent, handle: &ParamHandle) -> KeyOutcome {
    let mods = evt.modifiers();
    let step = wheel_step(mods) as f32;
    let outcome = match evt.key() {
        Key::ArrowUp | Key::ArrowRight => {
            nudge(handle, 1.0, step);
            KeyOutcome::Edited
        }
        Key::ArrowDown | Key::ArrowLeft => {
            nudge(handle, -1.0, step);
            KeyOutcome::Edited
        }
        Key::PageUp => {
            nudge(handle, 1.0, PAGE_STEP as f32);
            KeyOutcome::Edited
        }
        Key::PageDown => {
            nudge(handle, -1.0, PAGE_STEP as f32);
            KeyOutcome::Edited
        }
        Key::Home => {
            handle.set_as_gesture(0.0);
            KeyOutcome::Edited
        }
        Key::End => {
            handle.set_as_gesture(1.0);
            KeyOutcome::Edited
        }
        Key::Backspace | Key::Delete => {
            handle.reset_to_default();
            KeyOutcome::Edited
        }
        Key::Enter => KeyOutcome::OpenTextEntry,
        _ => KeyOutcome::Unhandled,
    };
    if outcome != KeyOutcome::Unhandled {
        evt.prevent_default();
    }
    outcome
}

// ── Typed values ──────────────────────────────────────────────────────────

/// Parse what the user typed into a readout, with the shared conventions,
/// then hand the normalized form to the parameter's own parser.
///
/// Conventions (`fx.control.text-entry.parse`):
/// - unit suffix optional, case-insensitive (`-6`, `-6dB`, `-6 db`);
/// - frequency: `1k` / `2.5k` / `1khz` → Hz; note names `A4`, `C#3`, `Bb2`,
///   `C#3+13` (cents) → Hz;
/// - dB: `2x` → +6.02 dB (ratio suffix);
/// - `N%` on a non-percent parameter → N % of the normalized range;
/// - out-of-range clamps (the handle's parser clamps).
///
/// Returns `None` only when nothing could be made of the text, so the caller
/// can flash the field rather than close it (`fx.control.text-entry`).
// r[impl fx.control.text-entry.parse]
pub fn parse_typed(handle: &ParamHandle, text: &str) -> Option<f32> {
    let raw = text.trim();
    if raw.is_empty() {
        return None;
    }
    let unit = handle.unit().to_ascii_lowercase();
    let lower = raw.to_ascii_lowercase();

    // `N%` on a non-percent parameter = N % of the normalized range.
    if !unit.contains('%') {
        if let Some(num) = lower.strip_suffix('%') {
            if let Ok(pct) = num.trim().parse::<f32>() {
                return Some((pct / 100.0).clamp(0.0, 1.0));
            }
        }
    }

    // Stepped parameters: accept an index as well as the label.
    if let Some(n) = handle.step_count() {
        if let Ok(idx) = lower.parse::<usize>() {
            if idx <= n {
                return Some(idx as f32 / n as f32);
            }
        }
    }

    let is_freq = unit.contains("hz");
    let is_db = unit.contains("db");

    // Candidates in order: the text verbatim, then each expansion.
    let mut candidates: Vec<String> = vec![raw.to_string()];

    if is_freq {
        if let Some(hz) = parse_khz(&lower) {
            candidates.push(format!("{hz}"));
        }
        if let Some(hz) = note_name_to_hz(raw) {
            candidates.push(format!("{hz:.3}"));
        }
    }
    if is_db {
        if let Some(num) = lower.strip_suffix('x') {
            if let Ok(ratio) = num.trim().parse::<f32>() {
                if ratio > 0.0 {
                    candidates.push(format!("{:.3}", 20.0 * ratio.log10()));
                }
            }
        }
    }
    // Strip the unit (any case, with or without a space) and retry bare.
    if !unit.is_empty() {
        if let Some(bare) = lower.strip_suffix(&unit) {
            candidates.push(bare.trim().to_string());
        }
    }
    // A bare number with a spurious unit the parser does not know — keep the
    // leading numeric run.
    let numeric: String = raw
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | ','))
        .collect();
    if !numeric.is_empty() && numeric != raw {
        candidates.push(numeric.replace(',', "."));
    }

    candidates
        .iter()
        .find_map(|c| handle.string_to_normalized(c))
}

/// `1k`, `2.5k`, `1khz`, `1 khz` → Hz.
fn parse_khz(lower: &str) -> Option<f32> {
    let s = lower.replace(' ', "");
    let s = s.strip_suffix("khz").or_else(|| s.strip_suffix('k'))?;
    s.parse::<f32>().ok().map(|k| k * 1000.0)
}

/// `A4` → 440, `C#3`, `Db3`, `C#3+13` (cents), `a4-10`. Case-insensitive.
pub fn note_name_to_hz(text: &str) -> Option<f32> {
    let s = text.trim();
    let mut chars = s.chars().peekable();
    let letter = chars.next()?.to_ascii_uppercase();
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut semis: i32 = base;
    // Accidental.
    if let Some(&c) = chars.peek() {
        match c {
            '#' | '♯' => {
                semis += 1;
                chars.next();
            }
            'b' | '♭' => {
                semis -= 1;
                chars.next();
            }
            _ => {}
        }
    }
    let rest: String = chars.collect();
    if rest.is_empty() {
        return None;
    }
    // Octave, then optional +/- cents.
    let split = rest
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i);
    let (oct_s, cents_s) = match split {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest.as_str(), ""),
    };
    let octave: i32 = oct_s.trim().parse().ok()?;
    let cents: f32 = if cents_s.is_empty() {
        0.0
    } else {
        cents_s.trim().parse().ok()?
    };
    let midi = (octave + 1) * 12 + semis;
    let hz = 440.0 * 2f32.powf((midi as f32 - 69.0 + cents / 100.0) / 12.0);
    Some(hz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn handle(unit: &str, steps: Option<usize>) -> (ParamHandle, Arc<Mutex<f32>>) {
        let v = Arc::new(Mutex::new(0.5f32));
        let g = v.clone();
        let s = v.clone();
        let h = ParamHandle::new(
            move || *g.lock().unwrap(),
            || {},
            move |x| *s.lock().unwrap() = x,
            || {},
            || String::new(),
            || "p".into(),
            // A plain "−24..+24 dB"-style linear parser: bare number → 0..1.
            |t| {
                t.trim()
                    .parse::<f32>()
                    .ok()
                    .map(|x| ((x + 24.0) / 48.0).clamp(0.0, 1.0))
            },
        )
        .with_unit(unit)
        .with_step_count(steps)
        .with_default(0.25);
        (h, v)
    }

    // r[verify fx.control.text-entry.parse]
    #[test]
    fn percent_of_range_on_non_percent_param() {
        let (h, _) = handle("dB", None);
        assert_eq!(parse_typed(&h, "50%"), Some(0.5));
        assert_eq!(parse_typed(&h, " 100 % "), Some(1.0));
    }

    // r[verify fx.control.text-entry.parse]
    #[test]
    fn ratio_suffix_on_db_param() {
        let (h, _) = handle("dB", None);
        let n = parse_typed(&h, "2x").unwrap();
        let db = n * 48.0 - 24.0;
        assert!((db - 6.02).abs() < 0.02, "{db}");
    }

    // r[verify fx.control.text-entry.parse]
    #[test]
    fn unit_suffix_optional_and_case_insensitive() {
        let (h, _) = handle("dB", None);
        assert_eq!(parse_typed(&h, "-6"), parse_typed(&h, "-6 DB"));
        assert_eq!(parse_typed(&h, "-6"), parse_typed(&h, "-6db"));
    }

    // r[verify fx.control.text-entry.parse]
    #[test]
    fn khz_and_note_names_on_frequency_param() {
        let v = Arc::new(Mutex::new(0.0f32));
        let s = v.clone();
        let h = ParamHandle::new(
            || 0.0,
            || {},
            move |x| *s.lock().unwrap() = x,
            || {},
            || String::new(),
            || "f".into(),
            |t| {
                t.trim()
                    .parse::<f32>()
                    .ok()
                    .map(|hz| (hz / 20_000.0).clamp(0.0, 1.0))
            },
        )
        .with_unit("Hz");
        let hz = |n: f32| n * 20_000.0;
        assert!((hz(parse_typed(&h, "1k").unwrap()) - 1000.0).abs() < 0.5);
        assert!((hz(parse_typed(&h, "2.5 kHz").unwrap()) - 2500.0).abs() < 0.5);
        assert!((hz(parse_typed(&h, "A4").unwrap()) - 440.0).abs() < 0.5);
        assert!((hz(parse_typed(&h, "a3").unwrap()) - 220.0).abs() < 0.5);
        assert!((hz(parse_typed(&h, "C#3+0").unwrap()) - 138.59).abs() < 0.5);
        // +100 cents on A4 is A#4.
        assert!((hz(parse_typed(&h, "A4+100").unwrap()) - 466.16).abs() < 0.5);
        assert_eq!(parse_typed(&h, "H4"), None);
    }

    // r[verify fx.control.stepped]
    #[test]
    fn stepped_params_snap_one_step() {
        let (h, v) = handle("", Some(3));
        *v.lock().unwrap() = 0.0;
        nudge(&h, 1.0, 0.02);
        assert!((*v.lock().unwrap() - 1.0 / 3.0).abs() < 1e-6);
        nudge(&h, 1.0, 0.02);
        assert!((*v.lock().unwrap() - 2.0 / 3.0).abs() < 1e-6);
        nudge(&h, -1.0, 0.02);
        assert!((*v.lock().unwrap() - 1.0 / 3.0).abs() < 1e-6);
        // Index typed in.
        assert!((parse_typed(&h, "3").unwrap() - 1.0).abs() < 1e-6);
    }

    // r[verify fx.control.wheel]
    #[test]
    fn continuous_nudge_moves_by_step_and_clamps() {
        let (h, v) = handle("", None);
        *v.lock().unwrap() = 0.99;
        nudge(&h, 1.0, 0.02);
        assert_eq!(*v.lock().unwrap(), 1.0);
        nudge(&h, -1.0, 0.02);
        assert!((*v.lock().unwrap() - 0.98).abs() < 1e-6);
    }

    // r[verify fx.control.fine]
    #[test]
    fn fine_ratios() {
        assert_eq!(fine_multiplier(Modifiers::empty()), 1.0);
        assert_eq!(fine_multiplier(Modifiers::CONTROL), FINE);
        assert_eq!(fine_multiplier(Modifiers::SHIFT), FINE);
        assert_eq!(fine_multiplier(Modifiers::META), FINE);
        assert_eq!(
            fine_multiplier(Modifiers::CONTROL | Modifiers::SHIFT),
            ULTRA_FINE
        );
    }
}
