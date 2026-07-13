//! Mixer workspace — the seeded Praise stems as a per-track mixer over the
//! in-process daw engine. Talks to the **backend-agnostic `daw` facade**
//! (`daw::get()` + the async `Project`/`TrackHandle` handles), NOT a
//! REAPER-specific handle, so the same mixer drives whatever backend is wired
//! (standalone here, REAPER later). Uses Dioxus async (`use_future`/`spawn`)
//! rather than `daw::block_on` — the UI thread already drives a runtime, so a
//! nested `block_on` would panic.

use dioxus::prelude::*;

use daw::service::Track;
use session_ui::components::MixerView;

/// Read the current project's tracks through the async daw facade.
async fn fetch_tracks() -> Vec<Track> {
    let Some(daw) = daw::get() else {
        return Vec::new();
    };
    let Ok(project) = daw.current_project().await else {
        return Vec::new();
    };
    project.tracks().all().await.unwrap_or_default()
}

/// Resolve a track handle by guid and run `f` on it (fire-and-forget edit).
async fn with_track<F, Fut>(guid: String, f: F)
where
    F: FnOnce(daw::rpc::TrackHandle) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if let Some(daw) = daw::get() {
        if let Ok(project) = daw.current_project().await {
            if let Ok(Some(th)) = project.tracks().by_guid(&guid).await {
                f(th).await;
            }
        }
    }
}

#[component]
pub fn MixerWorkspace() -> Element {
    let mut tracks = use_signal(Vec::<Track>::new);

    // Initial load of the seeded Praise stems.
    use_future(move || async move {
        tracks.set(fetch_tracks().await);
    });

    let on_volume = Callback::new(move |(guid, vol): (String, f64)| {
        spawn(async move {
            with_track(guid, |th| async move {
                let _ = th.set_volume(vol).await;
            })
            .await;
            tracks.set(fetch_tracks().await);
        });
    });
    let on_mute = Callback::new(move |guid: String| {
        spawn(async move {
            with_track(guid, |th| async move {
                let _ = th.toggle_mute().await;
            })
            .await;
            tracks.set(fetch_tracks().await);
        });
    });
    let on_solo = Callback::new(move |guid: String| {
        spawn(async move {
            with_track(guid, |th| async move {
                let _ = th.toggle_solo().await;
            })
            .await;
            tracks.set(fetch_tracks().await);
        });
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
