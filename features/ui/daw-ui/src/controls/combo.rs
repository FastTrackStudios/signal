//! The furniture both panels' combo fields share.
//!
//! The mixer's record-input field and the track panel's input combo show
//! the same fact and close with the same triangle. Each panel had its own
//! copy of both — the next `RecordInput` variant got added to one and not
//! the other, and the two carets were one refactor from disagreeing — so
//! the shared halves live here.

use daw_proto::Track;

use crate::prelude::*;

/// What a record-input field reads.
///
/// One match for both panels: the mixer's field and the track panel's
/// combo print the same name for the same input, and a new variant lands
/// in both at once.
pub fn record_input_name(track: &Track) -> String {
    use daw_proto::track::RecordInput;
    match track.record_input {
        RecordInput::None => "No input".to_string(),
        RecordInput::Audio { channel } => format!("Input {}", channel + 1),
        RecordInput::Midi { device_id, channel } => match (device_id, channel) {
            (Some(d), Some(c)) => format!("MIDI {d} ch {}", c + 1),
            (Some(d), None) => format!("MIDI {d}"),
            (None, Some(c)) => format!("MIDI all ch {}", c + 1),
            (None, None) => "MIDI".to_string(),
        },
        RecordInput::Raw(v) => format!("Input #{v}"),
    }
}

/// A dropdown's caret. A triangle rather than a glyph, so it does not
/// depend on a font having one. Positioned absolutely inside the field it
/// closes.
#[component]
pub fn Caret(x: f32, y: f32, ink: String) -> Element {
    rsx! {
        svg {
            style: "position:absolute; left:{x}px; top:{y}px;",
            width: "7", height: "4", view_box: "0 0 7 4",
            xmlns: "http://www.w3.org/2000/svg",
            path { d: "M 0 0 h 7 l -3.5 4 z", fill: "{ink}" }
        }
    }
}
