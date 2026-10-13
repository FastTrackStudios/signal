//! Song title display components for active and upcoming songs.

use crate::prelude::*;

/// Song title component displaying the currently active song
///
/// A pure UI component that displays song information.
/// Accepts the song name as a prop.
#[component]
pub fn SongTitle(song_name: String) -> Element {
    rsx! {
        // A label, not a hero. It used to be `text-6xl` centred with
        // two rems of air — a third of the view spent telling you what
        // you already knew you were playing. In the corner it says the
        // same thing and leaves the room for the work.
        div {
            class: "min-w-0",
            h1 {
                class: "text-2xl font-bold text-foreground truncate",
                "{song_name}"
            }
        }
    }
}

/// Faded song title component for displaying the next song
///
/// A pure UI component that displays a faded song title with custom color.
#[component]
pub fn FadedSongTitle(song_name: String, color: String) -> Element {
    rsx! {
        // On one line under the title, at the size of a caption: what
        // is coming is worth knowing and is not worth a headline.
        div {
            class: "flex items-baseline gap-2 min-w-0",
            span {
                class: "text-xs font-medium text-muted-foreground flex-none",
                "Next:"
            }
            span {
                class: "text-sm font-semibold truncate",
                style: format!("color: {}; opacity: 0.55;", color),
                "{song_name}"
            }
        }
    }
}
