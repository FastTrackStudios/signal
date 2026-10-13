//! Macro recording and playback support.

use crate::key::KeyChord;

#[derive(Debug, Clone)]
pub struct MacroRecorder {
    registers: [Vec<KeyChord>; 26],
    recording_register: Option<usize>,
    last_played: Option<usize>,
}

impl Default for MacroRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self {
            registers: std::array::from_fn(|_| Vec::new()),
            recording_register: None,
            last_played: None,
        }
    }

    pub fn start_recording(&mut self, register: char) -> bool {
        let Some(idx) = register_index(register) else {
            return false;
        };
        self.recording_register = Some(idx);
        self.registers[idx].clear();
        true
    }

    pub fn stop_recording(&mut self) {
        self.recording_register = None;
    }

    pub fn record(&mut self, chord: KeyChord) {
        if let Some(idx) = self.recording_register {
            self.registers[idx].push(chord);
        }
    }

    pub fn play(&mut self, register: char) -> Option<Vec<KeyChord>> {
        let idx = register_index(register)?;
        self.last_played = Some(idx);
        Some(self.registers[idx].clone())
    }

    pub fn play_last(&mut self) -> Option<Vec<KeyChord>> {
        let idx = self.last_played?;
        Some(self.registers[idx].clone())
    }

    pub fn is_recording(&self) -> bool {
        self.recording_register.is_some()
    }
}

fn register_index(register: char) -> Option<usize> {
    let lower = register.to_ascii_lowercase();
    if !lower.is_ascii_lowercase() {
        return None;
    }
    Some((lower as u8 - b'a') as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::KeyCode;

    fn chord(ch: char) -> KeyChord {
        KeyChord::plain(KeyCode::Character(ch.to_string()))
    }

    #[test]
    fn test_record_and_play_register() {
        let mut recorder = MacroRecorder::new();
        assert!(recorder.start_recording('a'));
        recorder.record(chord('j'));
        recorder.record(chord('j'));
        recorder.stop_recording();

        let seq = recorder.play('a').expect("register should exist");
        assert_eq!(seq.len(), 2);
    }

    #[test]
    fn test_invalid_register_rejected() {
        let mut recorder = MacroRecorder::new();
        assert!(!recorder.start_recording('1'));
        assert!(recorder.play('1').is_none());
    }

    #[test]
    fn test_play_last_register() {
        let mut recorder = MacroRecorder::new();
        recorder.start_recording('b');
        recorder.record(chord('k'));
        recorder.stop_recording();
        let _ = recorder.play('b');
        let seq = recorder.play_last().expect("has last");
        assert_eq!(seq.len(), 1);
    }
}
