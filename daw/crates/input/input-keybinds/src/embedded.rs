//! The FastTrackStudio profile, compiled in.
//!
//! An app installed away from a daw checkout has no profile directory to
//! read, and a keymap found relative to the working directory is found
//! only when the app happens to be started from the right place. So the
//! profile's files are embedded here, in the repo that owns them: a
//! consumer in another repo cannot `include_str!` across the boundary
//! (a git dependency has no stable path on disk), so it asks this crate
//! for the bytes instead.

use crate::loader::{Profile, load_profile_from};

/// Every file of `reaper-input/config/config/fasttrackstudio`, by name.
pub const FASTTRACKSTUDIO: &[(&str, &str)] = &[
    (
        "profile.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/profile.styx"
        ),
    ),
    (
        "transport.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/transport.styx"
        ),
    ),
    (
        "navigation.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/navigation.styx"
        ),
    ),
    (
        "modes.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/modes.styx"
        ),
    ),
    (
        "editing.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/editing.styx"
        ),
    ),
    (
        "grid.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/grid.styx"
        ),
    ),
    (
        "options.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/options.styx"
        ),
    ),
    (
        "tracks.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/tracks.styx"
        ),
    ),
    (
        "lanes-takes.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/lanes-takes.styx"
        ),
    ),
    (
        "visibility.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/visibility.styx"
        ),
    ),
    (
        "automation.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/automation.styx"
        ),
    ),
    (
        "fx.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/fx.styx"
        ),
    ),
    (
        "scrolling.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/scrolling.styx"
        ),
    ),
    (
        "zoom.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/zoom.styx"
        ),
    ),
    (
        "markers.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/markers.styx"
        ),
    ),
    (
        "views.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/views.styx"
        ),
    ),
    (
        "utility.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/utility.styx"
        ),
    ),
    (
        "midi-modes.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/midi-modes.styx"
        ),
    ),
    (
        "midi.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/midi.styx"
        ),
    ),
    (
        "expression-editor.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/expression-editor.styx"
        ),
    ),
    (
        "mouse.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/mouse.styx"
        ),
    ),
    (
        "mouse-profile.styx",
        include_str!(
            "../../../../features/reaper/reaper-input/config/config/fasttrackstudio/mouse-profile.styx"
        ),
    ),
];

/// The FastTrackStudio profile, from the embedded copy.
#[must_use]
pub fn fasttrackstudio() -> Option<Profile> {
    load_profile_from(|name| {
        FASTTRACKSTUDIO
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, text)| (*text).to_owned())
    })
}

#[cfg(test)]
mod tests {
    use input::parse_key_sequence;

    /// The embedded profile loads, carries the zoom tree, and names it.
    #[test]
    fn the_embedded_profile_has_the_zoom_tree_and_its_labels() {
        let profile = super::fasttrackstudio().expect("the embedded profile parses");
        let normal = profile.keymap.keymap.get("normal").expect("a normal mode");
        assert!(normal.contains_key("z t"), "z t is bound");
        let z = parse_key_sequence("z").unwrap();
        assert_eq!(profile.labels.get(&z).map(String::as_str), Some("Zoom"));
        let zt = parse_key_sequence("z t").unwrap();
        assert!(profile.labels.contains_key(&zt), "z t has a label");
    }
}
