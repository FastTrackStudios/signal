//! Mixer workspace — the seeded Praise stems as a per-track mixer over the
//! in-process daw-standalone engine. Reads/writes the `Tracks` service via the
//! `daw::reaper::Reaper` facade handle (sync, routed to the standalone backend)
//! and hands a flat track list to session-ui's presentational `MixerView`.

use dioxus::prelude::*;

use daw::reaper::Reaper;
use daw::service::{ProjectContext, Track, TrackRef, Tracks};
use session_ui::components::MixerView;

/// Refresh the track list from the current project. `Signal` is `Copy`, so we
/// pass it by value rather than sharing one `FnMut` closure.
fn reload(mut tracks: Signal<Vec<Track>>) {
    tracks.set(Tracks::all(&Reaper, ProjectContext::Current));
}

#[component]
pub fn MixerWorkspace() -> Element {
    let tracks = use_signal(Vec::<Track>::new);

    // Pull the current project's tracks (the seeded stems live here).
    use_effect(move || reload(tracks));

    let on_volume = Callback::new(move |(guid, vol): (String, f64)| {
        let _ = Tracks::set_volume(&Reaper, ProjectContext::Current, TrackRef::Guid(guid), vol);
        reload(tracks);
    });
    let on_mute = Callback::new(move |guid: String| {
        let cur = tracks
            .read()
            .iter()
            .find(|t| t.guid == guid)
            .map(|t| t.muted)
            .unwrap_or(false);
        let _ = Tracks::set_muted(&Reaper, ProjectContext::Current, TrackRef::Guid(guid), !cur);
        reload(tracks);
    });
    let on_solo = Callback::new(move |guid: String| {
        let cur = tracks
            .read()
            .iter()
            .find(|t| t.guid == guid)
            .map(|t| t.soloed)
            .unwrap_or(false);
        let _ = Tracks::set_soloed(&Reaper, ProjectContext::Current, TrackRef::Guid(guid), !cur);
        reload(tracks);
    });

    let list = tracks.read().clone();
    rsx! {
        div {
            style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            if list.is_empty() {
                div {
                    style: "padding: 24px; color: #a1a1aa; font-size: 13px;",
                    "No tracks yet — the Praise stems aren't seeded (audio folder not found on this machine, or the project has no tracks)."
                }
            } else {
                MixerView { tracks: list, on_volume, on_mute, on_solo }
            }
        }
    }
}
