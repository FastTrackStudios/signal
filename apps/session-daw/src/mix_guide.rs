//! Mix mode mutes the guide.
//!
//! The guide folder — click, count-in, cues, the prints — is for playing
//! to, not for mixing: going into Mix mode mutes a song's top-level
//! `Guide` folder, and coming out unmutes it again if Mix was what muted
//! it and it is still muted. A guide already muted is left muted, and one
//! unmuted while mixing stays as it was put.
//!
//! The song on screen, as it is shown: a song gone to in Mix mode is muted
//! then, and comes back when it is next shown in another mode.

use dioxus::prelude::*;
use session::modes::Mode;
use std::collections::HashMap;

/// One song's time in Mix mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stint {
    /// The song was last shown in Mix mode.
    mixing: bool,
    /// Mix mode muted its guide (so leaving unmutes it).
    muted_it: bool,
}

impl Stint {
    /// The song shown in a mode (`mixing` or not) with its guide muted or
    /// not: whether to toggle the guide's mute.
    const fn shown(&mut self, mixing: bool, guide_muted: bool) -> bool {
        if mixing && !self.mixing {
            self.mixing = true;
            self.muted_it = !guide_muted;
            return !guide_muted;
        }
        if !mixing && self.mixing {
            let undo = self.muted_it && guide_muted;
            *self = Self {
                mixing: false,
                muted_it: false,
            };
            return undo;
        }
        false
    }
}

/// The song's top-level `Guide` folder: guid, and whether it is muted.
fn guide(tracks: &[daw_proto::Track]) -> Option<(String, bool)> {
    tracks
        .iter()
        .find(|t| {
            t.is_folder && t.parent_guid.is_none() && t.name.trim().eq_ignore_ascii_case("guide")
        })
        .map(|t| (t.guid.clone(), t.muted))
}

/// For a shell, over its setlist and its mode: the song on screen's guide
/// muted in Mix mode, as above.
pub fn use_mix_guide(setlist: Signal<crate::setlist::Setlist>, mode: Signal<Mode>) {
    let applier = use_hook(|| std::rc::Rc::new(crate::engine::Applier::start()));
    let stints =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(HashMap::<String, Stint>::new())));
    use_effect(move || {
        let mixing = mode() == Mode::Mix;
        let Some((project, planner)) = setlist
            .read()
            .current()
            .map(|song| (song.project.clone(), song.session.planner.clone()))
        else {
            return;
        };
        let toggled = crate::widget::edit_song(&planner, |raw| {
            let (guid, muted) = guide(&raw.tracks)?;
            let toggle = stints
                .borrow_mut()
                .entry(project.clone())
                .or_default()
                .shown(mixing, muted);
            if toggle && let Some(track) = raw.tracks.iter_mut().find(|t| t.guid == guid) {
                track.muted = !track.muted;
            }
            toggle.then_some(guid)
        });
        if let Some(guid) = toggled {
            tracing::info!(
                mix.mixing = mixing,
                "mix mode: the guide folder's mute toggled"
            );
            if let Some(applier) = applier.as_ref() {
                applier.send(crate::engine::Edit::ToggleMute(guid));
            } else {
                tracing::debug!("no engine to mute the guide in");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::Stint;

    /// Into Mix the guide is muted; out of it, unmuted — if Mix muted it
    /// and it still is. Muted before, it stays muted; unmuted while
    /// mixing, it stays unmuted.
    #[test]
    fn mix_mode_mutes_the_guide_and_gives_it_back() {
        let mut stint = Stint::default();
        assert!(!stint.shown(false, false), "not mixing: nothing");
        assert!(stint.shown(true, false), "into Mix: muted");
        assert!(!stint.shown(true, true), "still mixing: nothing");
        assert!(stint.shown(false, true), "out of Mix: unmuted");

        let mut already = Stint::default();
        assert!(!already.shown(true, true), "muted before: left");
        assert!(!already.shown(false, true), "and left muted after");

        let mut unmuted = Stint::default();
        assert!(unmuted.shown(true, false));
        assert!(!unmuted.shown(false, false), "unmuted while mixing: left");
    }

    #[test]
    fn the_guide_is_the_top_level_guide_folder() {
        let track = |guid: &str, name: &str, folder: bool, parent: Option<&str>| daw_proto::Track {
            guid: guid.to_owned(),
            name: name.to_owned(),
            is_folder: folder,
            parent_guid: parent.map(str::to_owned),
            ..daw_proto::Track::default()
        };
        let tracks = [
            track("a", "Guide", false, None),
            track("b", "Guide", true, Some("x")),
            track("c", "GUIDE", true, None),
        ];
        assert_eq!(super::guide(&tracks), Some(("c".to_owned(), false)));
        assert_eq!(super::guide(&tracks[..2]), None);
    }
}
