//! Mirror the loaded drum kit onto a Komplete Kontrol Light Guide: each mapped
//! key glows a colour by piece type (kick red, snare yellow, hats green, toms
//! orange, cymbals blue/purple…), and flashes white when that note is played.
//!
//! Best-effort: [`DrumLightGuide::open`] returns `None` if no keyboard is
//! attached or hidraw isn't accessible (a udev rule / root), so the drum rig
//! runs fine without it.

use std::time::{Duration, Instant};

use kontrol::{KontrolUsb, LightColor};

/// Lowest / highest key of an 88-key keybed (A0..C8).
const LOW: u8 = 21;
const HIGH: u8 = 108;
const KEYS: usize = 88;
/// How long a struck key flashes white before returning to its base colour.
const FLASH: Duration = Duration::from_millis(140);

/// Colour for a drum piece, chosen by its engine-id keyword.
fn piece_color(id: &str) -> LightColor {
    let s = id.to_ascii_lowercase();
    if s.contains("kick") {
        LightColor::RED
    } else if s.contains("snare") {
        LightColor::YELLOW
    } else if s.contains("hh") || s.contains("hat") {
        LightColor::GREEN
    } else if s.contains("tom") {
        LightColor::ORANGE
    } else if s.contains("crash") {
        LightColor::BLUE
    } else if s.contains("ride") {
        LightColor::PURPLE
    } else if s.contains("china") {
        LightColor::PINK
    } else if s.contains("splash") {
        LightColor::new(LightColor::BLUE_BASE, kontrol::Intensity::Bright)
    } else {
        // Any other mapped note: dim white, so it's visibly "mapped".
        LightColor::new(LightColor::WHITE_BASE, kontrol::Intensity::Low)
    }
}

fn key_index(note: u8) -> Option<usize> {
    if (LOW..=HIGH).contains(&note) {
        Some((note - LOW) as usize)
    } else {
        None
    }
}

/// A live Light Guide reflecting the drum kit.
pub struct DrumLightGuide {
    kk: KontrolUsb,
    base: [LightColor; KEYS],
    flash_until: [Option<Instant>; KEYS],
    dirty: bool,
}

impl DrumLightGuide {
    /// Open the attached keyboard's Light Guide, or `None` if unavailable.
    pub fn open() -> Option<Self> {
        match KontrolUsb::open() {
            Ok(kk) => {
                tracing::info!("drum light guide: {}", kk.model());
                Some(Self {
                    kk,
                    base: [LightColor::OFF; KEYS],
                    flash_until: [None; KEYS],
                    dirty: true,
                })
            }
            Err(e) => {
                tracing::info!("drum light guide unavailable (no keyboard / permission): {e}");
                None
            }
        }
    }

    /// Set the base colours from the kit's pieces (`(note, engine_id)`).
    pub fn set_kit(&mut self, pieces: &[(u8, String)]) {
        self.base = [LightColor::OFF; KEYS];
        for (note, id) in pieces {
            if let Some(k) = key_index(*note) {
                self.base[k] = piece_color(id);
            }
        }
        self.dirty = true;
        self.render();
    }

    /// Flash a played note's key white; it decays back via [`tick`](Self::tick).
    pub fn note_on(&mut self, note: u8) {
        if let Some(k) = key_index(note) {
            self.flash_until[k] = Some(Instant::now() + FLASH);
            self.dirty = true;
            self.render();
        }
    }

    /// Expire finished flashes; call periodically (e.g. the meter pump).
    pub fn tick(&mut self) {
        let now = Instant::now();
        let mut changed = false;
        for f in self.flash_until.iter_mut() {
            if f.map(|t| now >= t).unwrap_or(false) {
                *f = None;
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
            self.render();
        }
    }

    /// All keys off.
    pub fn clear(&mut self) {
        self.base = [LightColor::OFF; KEYS];
        self.flash_until = [None; KEYS];
        self.dirty = true;
        self.render();
    }

    fn render(&mut self) {
        if !self.dirty {
            return;
        }
        for k in 0..KEYS {
            let c = if self.flash_until[k].is_some() {
                LightColor::WHITE
            } else {
                self.base[k]
            };
            self.kk.set_key(k as u8, c);
        }
        if let Err(e) = self.kk.flush() {
            tracing::debug!("light guide flush: {e}");
        }
        self.dirty = false;
    }
}
