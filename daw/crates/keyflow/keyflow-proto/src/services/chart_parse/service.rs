//! Generic chart parsing — text or MIDI sources.

#[architect::rpc]
pub trait ChartParsers {
    fn parse_text(&self, text: String) -> Result<crate::Chart, String>;
    fn parse_midi(&self, bytes: Vec<u8>) -> Result<crate::Chart, String>;
}
