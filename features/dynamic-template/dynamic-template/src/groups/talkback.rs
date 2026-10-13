//! Talkback group definitions.
//!
//! Talkback is the engineer's and players' comms — a mic in the control room,
//! a mic on each player's stand — recorded alongside the music so takes can be
//! reviewed with the conversation intact. It is **not** content: a talkback mic
//! on the bass player is not bass, and summing it into the mix prints room
//! chatter over the record.
//!
//! The convention in these sessions is a `TB` prefix naming whose talkback it
//! is (`TB Bass`, `TB Drums`, `TB Engineer`), which is why this group has to
//! exist rather than letting `TB Bass` classify as bass — it matched the
//! instrument, and routed the chatter into the mix.

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level talkback group for comms mics.
pub struct Talkback;

impl From<Talkback> for Group<ItemMetadata> {
    fn from(_val: Talkback) -> Self {
        Self::builder("Talkback")
            .prefix("TB")
            .patterns(vec![
                "talkback",
                "talk back",
                "talk-back",
                // The house prefix. Bare "tb" only ever means talkback here;
                // it is not an abbreviation any instrument uses.
                "tb",
                "comms",
                "intercom",
                "cue mic",
                "engineer mic",
            ])
            .build()
    }
}

#[cfg(test)]
mod tests {
    use crate::track_schema::classify_track;

    fn groups(name: &str) -> Vec<String> {
        classify_track(name).matched_groups
    }

    /// The bug this group exists for: `TB Bass` classified as bass and was
    /// routed into the mix.
    #[test]
    fn tb_prefixed_tracks_are_talkback_not_the_instrument_named() {
        for name in ["TB Bass", "TB Drums", "TB Keys", "TB Guitar"] {
            let g = groups(name);
            assert!(
                g.iter().any(|x| x == "Talkback"),
                "{name} should be Talkback, got {g:?}"
            );
        }
    }

    #[test]
    fn bare_talkback_names_classify() {
        for name in ["Talkback", "TB Engineer", "Talkback Mic"] {
            assert!(
                groups(name).iter().any(|x| x == "Talkback"),
                "{name} should be Talkback"
            );
        }
    }

    /// The prefix must not swallow real instruments whose names merely contain
    /// the letters.
    #[test]
    fn instruments_are_not_mistaken_for_talkback() {
        for name in ["Bass DI", "Kick In", "GTR E - Chords", "Tuba", "Timbales"] {
            let g = groups(name);
            assert!(
                !g.iter().any(|x| x == "Talkback"),
                "{name} should not be Talkback, got {g:?}"
            );
        }
    }
}
