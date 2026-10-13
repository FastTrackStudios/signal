//! Mackie Control Universal protocol codec, with the Behringer
//! X-Touch extensions (scribble-strip colors).
//!
//! Pure functions over byte slices — no I/O, no state. The driver
//! decodes incoming MIDI into [`SurfaceInput`] and renders feedback
//! through the `encode_*` builders. Wire layout follows the de-facto
//! MCU standard (the same map CSI's X-Touch.mst uses):
//!
//! - Faders: pitch-bend per channel (0–7 strips, 8 = master), 14-bit
//! - Fader touch: notes `0x68 + strip` (0x70 = master)
//! - V-Pots: relative CC `0x10 + strip` in (sign-magnitude), LED ring
//!   CC `0x30 + strip` out
//! - Buttons: notes (see [`Button`]); LED feedback echoes the note
//!   with velocity 0x7F / 0x00
//! - Meters: channel pressure, value = `strip << 4 | level`
//! - Scribble strips: Mackie LCD sysex `F0 00 00 66 14 12 pos .. F7`
//!   (7 chars per strip, top row offset 0, bottom row offset 56)
//! - X-Touch strip colors: `F0 00 00 66 14 72 c0..c7 F7`

/// Number of channel strips on an X-Touch / MCU main unit.
pub const STRIPS: usize = 8;

/// Strip index meaning "the master fader" in fader messages.
pub const MASTER: u8 = 8;

// ── Input decoding ──────────────────────────────────────────────────

/// One decoded gesture from the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceInput {
    /// Fader moved. `strip` 0–7, or [`MASTER`]. `pos` is the raw
    /// 14-bit position (0–16383).
    Fader { strip: u8, pos: u16 },
    /// Fader touched / released (motorized fader touch sensor).
    FaderTouch { strip: u8, touched: bool },
    /// V-Pot rotated. Negative = counter-clockwise. Magnitude is the
    /// surface's acceleration (1 = slow tick).
    VPot { strip: u8, delta: i8 },
    /// V-Pot pushed.
    VPotPress { strip: u8, pressed: bool },
    /// Jog wheel. Negative = counter-clockwise.
    Jog { delta: i8 },
    /// A named button.
    Button { button: Button, pressed: bool },
}

/// MCU button map (note numbers). Strip buttons carry their strip
/// index; everything else is positional on the master section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Rec(u8),
    Solo(u8),
    Mute(u8),
    Select(u8),
    /// Track / Pan / Send / Plugin / EQ / Inst assignment keys.
    Assign(AssignKey),
    BankLeft,
    BankRight,
    ChannelLeft,
    ChannelRight,
    Flip,
    GlobalView,
    /// Display buttons: NAME/VALUE (0x34) and SMPTE/BEATS (0x35).
    NameValue,
    SmpteBeats,
    Shift,
    Option,
    Control,
    Alt,
    /// F1–F8 (0-based).
    Function(u8),
    /// Automation cluster: READ/WRITE/TRIM/TOUCH/LATCH/GROUP.
    AutoRead,
    AutoWrite,
    AutoTrim,
    AutoTouch,
    AutoLatch,
    AutoGroup,
    Save,
    Undo,
    Cancel,
    Enter,
    Marker,
    Nudge,
    Cycle,
    Drop,
    Replace,
    Click,
    SoloGlobal,
    Rewind,
    FastForward,
    Stop,
    Play,
    Record,
    Up,
    Down,
    Left,
    Right,
    Zoom,
    Scrub,
    /// Anything we don't name yet — raw note number.
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignKey {
    Track,
    Send,
    Pan,
    Plugin,
    Eq,
    Inst,
}

fn button_from_note(note: u8) -> Button {
    match note {
        0x00..=0x07 => Button::Rec(note),
        0x08..=0x0F => Button::Solo(note - 0x08),
        0x10..=0x17 => Button::Mute(note - 0x10),
        0x18..=0x1F => Button::Select(note - 0x18),
        0x28 => Button::Assign(AssignKey::Track),
        0x29 => Button::Assign(AssignKey::Send),
        0x2A => Button::Assign(AssignKey::Pan),
        0x2B => Button::Assign(AssignKey::Plugin),
        0x2C => Button::Assign(AssignKey::Eq),
        0x2D => Button::Assign(AssignKey::Inst),
        0x2E => Button::BankLeft,
        0x2F => Button::BankRight,
        0x30 => Button::ChannelLeft,
        0x31 => Button::ChannelRight,
        0x32 => Button::Flip,
        0x33 => Button::GlobalView,
        0x34 => Button::NameValue,
        0x35 => Button::SmpteBeats,
        0x36..=0x3D => Button::Function(note - 0x36),
        0x46 => Button::Shift,
        0x47 => Button::Option,
        0x48 => Button::Control,
        0x49 => Button::Alt,
        0x4A => Button::AutoRead,
        0x4B => Button::AutoWrite,
        0x4C => Button::AutoTrim,
        0x4D => Button::AutoTouch,
        0x4E => Button::AutoLatch,
        0x4F => Button::AutoGroup,
        0x50 => Button::Save,
        0x51 => Button::Undo,
        0x52 => Button::Cancel,
        0x53 => Button::Enter,
        0x54 => Button::Marker,
        0x55 => Button::Nudge,
        0x56 => Button::Cycle,
        0x57 => Button::Drop,
        0x58 => Button::Replace,
        0x59 => Button::Click,
        0x5A => Button::SoloGlobal,
        0x5B => Button::Rewind,
        0x5C => Button::FastForward,
        0x5D => Button::Stop,
        0x5E => Button::Play,
        0x5F => Button::Record,
        0x60 => Button::Up,
        0x61 => Button::Down,
        0x62 => Button::Left,
        0x63 => Button::Right,
        0x64 => Button::Zoom,
        0x65 => Button::Scrub,
        n => Button::Other(n),
    }
}

/// Note number for a button (inverse of the input decode mapping),
/// used to address its LED.
pub fn note_for_button(button: Button) -> u8 {
    match button {
        Button::Rec(s) => s,
        Button::Solo(s) => 0x08 + s,
        Button::Mute(s) => 0x10 + s,
        Button::Select(s) => 0x18 + s,
        Button::Assign(AssignKey::Track) => 0x28,
        Button::Assign(AssignKey::Send) => 0x29,
        Button::Assign(AssignKey::Pan) => 0x2A,
        Button::Assign(AssignKey::Plugin) => 0x2B,
        Button::Assign(AssignKey::Eq) => 0x2C,
        Button::Assign(AssignKey::Inst) => 0x2D,
        Button::BankLeft => 0x2E,
        Button::BankRight => 0x2F,
        Button::ChannelLeft => 0x30,
        Button::ChannelRight => 0x31,
        Button::Flip => 0x32,
        Button::GlobalView => 0x33,
        Button::NameValue => 0x34,
        Button::SmpteBeats => 0x35,
        Button::Function(f) => 0x36 + f,
        Button::Shift => 0x46,
        Button::Option => 0x47,
        Button::Control => 0x48,
        Button::Alt => 0x49,
        Button::AutoRead => 0x4A,
        Button::AutoWrite => 0x4B,
        Button::AutoTrim => 0x4C,
        Button::AutoTouch => 0x4D,
        Button::AutoLatch => 0x4E,
        Button::AutoGroup => 0x4F,
        Button::Save => 0x50,
        Button::Undo => 0x51,
        Button::Cancel => 0x52,
        Button::Enter => 0x53,
        Button::Marker => 0x54,
        Button::Nudge => 0x55,
        Button::Cycle => 0x56,
        Button::Drop => 0x57,
        Button::Replace => 0x58,
        Button::Click => 0x59,
        Button::SoloGlobal => 0x5A,
        Button::Rewind => 0x5B,
        Button::FastForward => 0x5C,
        Button::Stop => 0x5D,
        Button::Play => 0x5E,
        Button::Record => 0x5F,
        Button::Up => 0x60,
        Button::Down => 0x61,
        Button::Left => 0x62,
        Button::Right => 0x63,
        Button::Zoom => 0x64,
        Button::Scrub => 0x65,
        Button::Other(n) => n,
    }
}

/// Decode one raw MIDI message from the surface. Returns `None` for
/// messages we don't understand (or feedback echoes).
pub fn decode(raw: &[u8]) -> Option<SurfaceInput> {
    match raw {
        // Pitch bend = fader position. Channel 0–7 strips, 8 master.
        [st @ 0xE0..=0xE8, lsb, msb] => Some(SurfaceInput::Fader {
            strip: st - 0xE0,
            pos: ((*msb as u16) << 7) | (*lsb as u16),
        }),
        // Note on/off — touch sensors, v-pot presses, buttons.
        // The X-Touch sends note-on velocity 0 for release.
        [0x90 | 0x80, note, vel] => {
            let pressed = raw[0] == 0x90 && *vel > 0;
            match note {
                0x68..=0x70 => Some(SurfaceInput::FaderTouch {
                    strip: note - 0x68,
                    touched: pressed,
                }),
                0x20..=0x27 => Some(SurfaceInput::VPotPress {
                    strip: note - 0x20,
                    pressed,
                }),
                _ => Some(SurfaceInput::Button {
                    button: button_from_note(*note),
                    pressed,
                }),
            }
        }
        // CC — v-pot rotation (0x10–0x17) and jog (0x3C), both
        // sign-magnitude relative: bit 6 set = counter-clockwise.
        [0xB0, cc @ 0x10..=0x17, val] => Some(SurfaceInput::VPot {
            strip: cc - 0x10,
            delta: sign_magnitude(*val),
        }),
        [0xB0, 0x3C, val] => Some(SurfaceInput::Jog {
            delta: sign_magnitude(*val),
        }),
        _ => None,
    }
}

fn sign_magnitude(val: u8) -> i8 {
    let mag = (val & 0x3F) as i8;
    if val & 0x40 != 0 { -mag } else { mag }
}

// ── Output encoding ─────────────────────────────────────────────────

/// Motor fader position. `strip` 0–7 or [`MASTER`]; `pos` 0–16383.
pub fn encode_fader(strip: u8, pos: u16) -> [u8; 3] {
    [0xE0 + strip, (pos & 0x7F) as u8, (pos >> 7) as u8]
}

/// V-Pot LED ring display modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingMode {
    /// One LED at the value position.
    SingleDot = 0,
    /// Fill from center toward the value (pan-style boost/cut).
    BoostCut = 1,
    /// Fill from the left edge (level-style).
    Wrap = 2,
    /// Symmetric spread from center (width-style).
    Spread = 3,
}

/// V-Pot LED ring. `value` 0 blanks the ring; 1–11 light positions.
/// `center_led` is the small LED under the pot.
pub fn encode_vpot_ring(strip: u8, mode: RingMode, value: u8, center_led: bool) -> [u8; 3] {
    let v = (value.min(11)) | ((mode as u8) << 4) | if center_led { 0x40 } else { 0 };
    [0xB0, 0x30 + strip, v]
}

/// Button LED on/off (echo the button's note with velocity).
pub fn encode_button_led(button: Button, lit: bool) -> [u8; 3] {
    [0x90, note_for_button(button), if lit { 0x7F } else { 0x00 }]
}

/// Channel meter. `level` 0–12 (0x0C = clip). Sent as channel
/// pressure; the surface decays the bar on its own, so the driver
/// only sends rises.
pub fn encode_meter(strip: u8, level: u8) -> [u8; 2] {
    [0xD0, (strip << 4) | level.min(0x0E)]
}

/// Scribble strip text. `row` 0 = top, 1 = bottom. Text is clamped /
/// space-padded to 7 chars, ASCII-sanitized (the LCD is 7-bit).
pub fn encode_lcd(strip: u8, row: u8, text: &str) -> Vec<u8> {
    let pos = strip as usize * 7 + row as usize * 56;
    let mut msg = vec![0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, pos as u8];
    let mut chars = text
        .chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() {
                c as u8
            } else {
                b'?'
            }
        })
        .take(7)
        .collect::<Vec<u8>>();
    chars.resize(7, b' ');
    msg.extend_from_slice(&chars);
    msg.push(0xF7);
    msg
}

/// X-Touch scribble strip colors (one sysex sets all 8).
pub fn encode_strip_colors(colors: [StripColor; STRIPS]) -> Vec<u8> {
    let mut msg = vec![0xF0, 0x00, 0x00, 0x66, 0x14, 0x72];
    msg.extend(colors.iter().map(|c| *c as u8));
    msg.push(0xF7);
    msg
}

/// The X-Touch's 8 scribble strip backlight colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StripColor {
    Off = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    #[default]
    White = 7,
}

/// Quantize a 0xRRGGBB track color onto the X-Touch palette. Port of
/// CSI's `rgbToColor` (RGB → HSV, low-sat/low-val → white, then hue
/// bands).
pub fn rgb_to_strip_color(rgb: u32) -> StripColor {
    let r = ((rgb >> 16) & 0xFF) as f64 / 255.0;
    let g = ((rgb >> 8) & 0xFF) as f64 / 255.0;
    let b = (rgb & 0xFF) as f64 / 255.0;
    let v = r.max(g).max(b);
    if v <= 0.10 {
        return StripColor::White;
    }
    let min = r.min(g).min(b);
    let delta = v - min;
    let s = delta / v;
    if s <= 0.10 {
        return StripColor::White;
    }
    let mut h = if r >= v {
        (g - b) / delta
    } else if g >= v {
        2.0 + (b - r) / delta
    } else {
        4.0 + (r - g) / delta
    } * 60.0;
    if h < 0.0 {
        h += 360.0;
    }
    match h {
        h if !(20.0..330.0).contains(&h) => StripColor::Red,
        h if h >= 250.0 => StripColor::Magenta,
        h if h >= 210.0 => StripColor::Blue,
        h if h >= 160.0 => StripColor::Cyan,
        h if h >= 80.0 => StripColor::Green,
        h if h >= 20.0 => StripColor::Yellow,
        _ => StripColor::White,
    }
}

/// MCU 7-segment timecode: 10 digit registers driven by CC 0x40
/// (rightmost) … 0x49 (leftmost). Value = 6-bit char code; bit 6
/// lights the digit's decimal point. `text` may contain '.' after a
/// digit to set its dot; everything right-aligns.
pub fn encode_timecode(text: &str) -> Vec<[u8; 3]> {
    // Fold "12.34" into [(1,false),(2,true),(3,false),(4,false)].
    let mut cells: Vec<(u8, bool)> = Vec::new();
    for c in text.chars() {
        if c == '.' {
            if let Some(last) = cells.last_mut() {
                last.1 = true;
            }
        } else if c.is_ascii() && !c.is_ascii_control() {
            cells.push((c as u8, false));
        }
    }
    // Right-align into the 10 digit registers.
    let mut out = Vec::with_capacity(10);
    for i in 0..10usize {
        let (ch, dot) = cells
            .len()
            .checked_sub(i + 1)
            .and_then(|idx| cells.get(idx).copied())
            .unwrap_or((b' ', false));
        let value = (ch & 0x3F) | if dot { 0x40 } else { 0 };
        out.push([0xB0, 0x40 + i as u8, value]);
    }
    out
}

/// Pan (−1..1) to v-pot ring position 1–11 (6 = center).
pub fn pan_to_ring(pan: f64) -> u8 {
    let p = pan.clamp(-1.0, 1.0);
    (6.0 + p * 5.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fader_roundtrip() {
        for pos in [0u16, 1, 8192, 16383] {
            for strip in [0u8, 3, MASTER] {
                let raw = encode_fader(strip, pos);
                assert_eq!(decode(&raw), Some(SurfaceInput::Fader { strip, pos }));
            }
        }
    }

    #[test]
    fn vpot_sign_magnitude() {
        assert_eq!(
            decode(&[0xB0, 0x10, 0x01]),
            Some(SurfaceInput::VPot { strip: 0, delta: 1 })
        );
        assert_eq!(
            decode(&[0xB0, 0x17, 0x43]),
            Some(SurfaceInput::VPot {
                strip: 7,
                delta: -3
            })
        );
        assert_eq!(
            decode(&[0xB0, 0x3C, 0x41]),
            Some(SurfaceInput::Jog { delta: -1 })
        );
    }

    #[test]
    fn buttons_decode_and_leds_roundtrip() {
        // Strip buttons carry their index.
        assert_eq!(
            decode(&[0x90, 0x08, 0x7F]),
            Some(SurfaceInput::Button {
                button: Button::Solo(0),
                pressed: true
            })
        );
        // X-Touch note-on velocity 0 = release.
        assert_eq!(
            decode(&[0x90, 0x5E, 0x00]),
            Some(SurfaceInput::Button {
                button: Button::Play,
                pressed: false
            })
        );
        // LED encode echoes the same note.
        for b in [
            Button::Rec(7),
            Button::Mute(0),
            Button::Select(3),
            Button::Cycle,
            Button::Play,
            Button::GlobalView,
        ] {
            let led = encode_button_led(b, true);
            assert_eq!(button_from_note(led[1]), b);
        }
    }

    #[test]
    fn fader_touch() {
        assert_eq!(
            decode(&[0x90, 0x68, 0x7F]),
            Some(SurfaceInput::FaderTouch {
                strip: 0,
                touched: true
            })
        );
        assert_eq!(
            decode(&[0x90, 0x70, 0x00]),
            Some(SurfaceInput::FaderTouch {
                strip: 8,
                touched: false
            })
        );
    }

    #[test]
    fn lcd_layout() {
        // Strip 2, bottom row → offset 2*7 + 56 = 70; 7 chars padded.
        let msg = encode_lcd(2, 1, "Kick");
        assert_eq!(&msg[..7], &[0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 70]);
        assert_eq!(&msg[7..14], b"Kick   ");
        assert_eq!(msg[14], 0xF7);
        // Non-ASCII → '?', overlong → clamped.
        let msg = encode_lcd(0, 0, "Tomtom Überlong");
        assert_eq!(&msg[7..14], b"Tomtom ");
    }

    #[test]
    fn strip_colors() {
        assert_eq!(rgb_to_strip_color(0xFF0000), StripColor::Red);
        assert_eq!(rgb_to_strip_color(0x00FF00), StripColor::Green);
        assert_eq!(rgb_to_strip_color(0x0000FF), StripColor::Blue);
        assert_eq!(rgb_to_strip_color(0xFFFF00), StripColor::Yellow);
        assert_eq!(rgb_to_strip_color(0x808080), StripColor::White); // gray → white
        assert_eq!(rgb_to_strip_color(0x000000), StripColor::White); // black → white
        let msg = encode_strip_colors([StripColor::Red; STRIPS]);
        assert_eq!(msg.len(), 6 + 8 + 1);
        assert_eq!(msg[5], 0x72);
    }

    #[test]
    fn pan_ring() {
        assert_eq!(pan_to_ring(0.0), 6);
        assert_eq!(pan_to_ring(-1.0), 1);
        assert_eq!(pan_to_ring(1.0), 11);
    }
}
