//! The Light Guide on a phone or tablet: there is no keyboard to light, so
//! [`DrumLightGuide::open`] finds none and the rig goes on without it.

/// Never opened here; the methods are the desktop guide's, doing nothing.
pub struct DrumLightGuide;

impl DrumLightGuide {
    /// No keyboard on this device.
    #[must_use]
    pub fn open() -> Option<Self> {
        None
    }
    pub fn set_kit(&mut self, _pieces: &[(u8, String)]) {}
    pub fn note_on(&mut self, _note: u8) {}
    pub fn tick(&mut self) {}
    pub fn clear(&mut self) {}
}
