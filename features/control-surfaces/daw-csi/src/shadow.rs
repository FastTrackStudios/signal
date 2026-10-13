//! Shadow of the surface's output state. Every feedback path diffs
//! against this before sending — motor faders don't twitch, the LCD
//! isn't rewritten per event, and a full refresh after banking only
//! sends what actually changed.

use crate::mcu::{self, Button, RingMode, StripColor};

pub struct Shadow {
    /// 14-bit fader positions; index 8 = master.
    faders: [Option<u16>; 9],
    /// V-Pot ring raw value byte per strip.
    rings: [Option<u8>; mcu::STRIPS],
    /// Button LED states by note number.
    leds: [Option<bool>; 128],
    /// 7-char LCD cells: \[strip]\[row].
    lcd: [[Option<[u8; 7]>; 2]; mcu::STRIPS],
    colors: Option<[StripColor; mcu::STRIPS]>,
    /// Last meter level sent per strip (the surface decays on its
    /// own; we resend only on rise or after a refresh).
    meters: [u8; mcu::STRIPS],
    /// 7-segment timecode digit registers (CC 0x40–0x49 values).
    timecode: [Option<u8>; 10],
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            faders: [None; 9],
            rings: [None; mcu::STRIPS],
            leds: [None; 128],
            lcd: [[None; 2]; mcu::STRIPS],
            colors: None,
            meters: [0; mcu::STRIPS],
            timecode: [None; 10],
        }
    }
}

impl Shadow {
    /// Forget everything — next updates resend unconditionally. Call
    /// on connect.
    pub fn invalidate(&mut self) {
        *self = Self::default();
    }

    /// Queue a fader move if the position changed.
    pub fn fader(&mut self, out: &mut Vec<Vec<u8>>, strip: u8, pos: u16) {
        let cell = &mut self.faders[strip as usize];
        if *cell != Some(pos) {
            *cell = Some(pos);
            out.push(mcu::encode_fader(strip, pos).to_vec());
        }
    }

    /// Queue a v-pot ring update if changed.
    pub fn ring(
        &mut self,
        out: &mut Vec<Vec<u8>>,
        strip: u8,
        mode: RingMode,
        value: u8,
        center: bool,
    ) {
        let msg = mcu::encode_vpot_ring(strip, mode, value, center);
        let cell = &mut self.rings[strip as usize];
        if *cell != Some(msg[2]) {
            *cell = Some(msg[2]);
            out.push(msg.to_vec());
        }
    }

    /// Queue a button LED change.
    pub fn led(&mut self, out: &mut Vec<Vec<u8>>, button: Button, lit: bool) {
        let note = mcu::note_for_button(button) as usize;
        if self.leds[note] != Some(lit) {
            self.leds[note] = Some(lit);
            out.push(mcu::encode_button_led(button, lit).to_vec());
        }
    }

    /// Queue an LCD cell rewrite if the text changed.
    pub fn lcd(&mut self, out: &mut Vec<Vec<u8>>, strip: u8, row: u8, text: &str) {
        let msg = mcu::encode_lcd(strip, row, text);
        let mut cell = [0u8; 7];
        cell.copy_from_slice(&msg[7..14]);
        let slot = &mut self.lcd[strip as usize][row as usize];
        if *slot != Some(cell) {
            *slot = Some(cell);
            out.push(msg);
        }
    }

    /// Queue the strip-color sysex if any color changed.
    pub fn colors(&mut self, out: &mut Vec<Vec<u8>>, colors: [StripColor; mcu::STRIPS]) {
        if self.colors != Some(colors) {
            self.colors = Some(colors);
            out.push(mcu::encode_strip_colors(colors));
        }
    }

    /// Queue 7-segment timecode digits that changed.
    pub fn timecode(&mut self, out: &mut Vec<Vec<u8>>, text: &str) {
        for msg in mcu::encode_timecode(text) {
            let i = (msg[1] - 0x40) as usize;
            if self.timecode[i] != Some(msg[2]) {
                self.timecode[i] = Some(msg[2]);
                out.push(msg.to_vec());
            }
        }
    }

    /// Queue a meter update. The surface decays the bar itself, so
    /// only rises are sent; `level` 0 just lets it fall.
    pub fn meter(&mut self, out: &mut Vec<Vec<u8>>, strip: u8, level: u8) {
        let cell = &mut self.meters[strip as usize];
        if level > *cell {
            out.push(mcu::encode_meter(strip, level).to_vec());
        }
        *cell = level;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffs_suppress_repeats() {
        let mut s = Shadow::default();
        let mut out = Vec::new();
        s.fader(&mut out, 0, 8000);
        s.fader(&mut out, 0, 8000);
        assert_eq!(out.len(), 1, "identical fader position resent");
        s.fader(&mut out, 0, 8001);
        assert_eq!(out.len(), 2);

        out.clear();
        s.lcd(&mut out, 3, 0, "Kick");
        s.lcd(&mut out, 3, 0, "Kick");
        assert_eq!(out.len(), 1);
        s.lcd(&mut out, 3, 0, "Kick 2");
        assert_eq!(out.len(), 2);

        out.clear();
        s.led(&mut out, Button::Mute(2), true);
        s.led(&mut out, Button::Mute(2), true);
        assert_eq!(out.len(), 1);

        out.clear();
        s.invalidate();
        s.fader(&mut out, 0, 8001);
        assert_eq!(out.len(), 1, "invalidate must force resend");
    }

    #[test]
    fn meters_only_rise() {
        let mut s = Shadow::default();
        let mut out = Vec::new();
        s.meter(&mut out, 0, 5);
        s.meter(&mut out, 0, 3); // falling — surface decays itself
        s.meter(&mut out, 0, 9); // rising — send
        assert_eq!(out.len(), 2);
    }
}
